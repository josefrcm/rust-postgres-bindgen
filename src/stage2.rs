use crate::types::*;
use heck::{CamelCase, SnakeCase};

// TODO: generate type aliases for domains

// --------------------------------------------------------------------------------------------------------------------
// Public functions
// --------------------------------------------------------------------------------------------------------------------

/// Generate the module definitions
pub fn run(database: PgDatabase) -> codegen::Scope {
    let mut scope = codegen::Scope::new();

    scope.import("enum_map", "*");
    scope.import("super", "{PgConnection, PgResult}");
    scope.import("postgres_mapper", "FromPostgresRow");

    // Generate the types
    for (_oid, type_def) in &database.types {
        if let Err(e) = gen_type(&mut scope, &database.types, type_def) {
            eprintln!("{}", e);
            eprintln!("{:#?}", type_def);
        }
    }

    // Generate the functions
    let fimpl = scope.new_impl("PgConnection");
    for (_oid, func_def) in &database.functions {
        match gen_function(&database.types, func_def) {
            Ok(new_func) => {
                fimpl.push_fn(new_func);
            }
            Err(e) => {
                eprintln!("{}", e);
                eprintln!("{:#?}", func_def);
            }
        }
    }

    // Done
    scope
}

// --------------------------------------------------------------------------------------------------------------------
// Private functions
// --------------------------------------------------------------------------------------------------------------------

fn gen_type(scope: &mut codegen::Scope, database: &BTreeMap<Oid, PgType>, type_def: &PgType) -> Result<(), String> {
    match type_def {
        PgType::Enum { schema, name, values } => gen_enum(scope, &schema, &name, &values),
        PgType::Composite {
            schema,
            name,
            is_table,
            is_view,
            fields,
        } => gen_composite(scope, &database, &schema, &name, *is_table, *is_view, &fields),
        _ => Ok(()),
    }
}

fn gen_enum(scope: &mut codegen::Scope, schema: &String, name: &String, values: &Vec<String>) -> Result<(), String> {
    // Create the new enum definition
    let new_enum = scope.new_enum(&gen_type_name(schema, name));

    // Make it public
    new_enum.vis("pub");

    // Add the derives
    new_enum.derive("Debug");
    new_enum.derive("Copy");
    new_enum.derive("Clone");
    new_enum.derive("PartialEq");
    new_enum.derive("Eq");
    new_enum.derive("PartialOrd");
    new_enum.derive("Ord");
    new_enum.derive("Serialize");
    new_enum.derive("Deserialize");
    new_enum.derive("ToSql");
    new_enum.derive("FromSql");
    new_enum.derive("FromFormValue");
    new_enum.derive("Enum");

    // Add the annotation
    //new_enum.r#macro(&format!("#[postgres(name = \"{}.{}\")]", schema, name));
    new_enum.r#macro(&format!("#[postgres(name = \"{}\")]", name));

    // Generate the enum values
    for pg_value in values {
        let rs_value = pg_value.to_camel_case();
        let variant = new_enum.new_variant(&rs_value);
        if !rs_value.eq(pg_value) {
            variant.annotation(&format!("#[postgres(name = \"{}\")]", pg_value));
        }
    }
    Ok(())
}

fn gen_composite(
    scope: &mut codegen::Scope,
    database: &BTreeMap<Oid, PgType>,
    schema: &String,
    name: &String,
    is_table: bool,
    is_view: bool,
    fields: &Vec<PgField>,
) -> Result<(), String> {
    // Create the new struct definition
    let new_struct = scope.new_struct(&gen_type_name(schema, name));

    // Make it public
    new_struct.vis("pub");

    // Add the derives
    new_struct.derive("Debug");
    new_struct.derive("Clone");

    // Add the annotations
    if is_table || is_view {
        new_struct.derive("PostgresMapper");
        new_struct.r#macro(&format!("#[pg_mapper(table = \"{}\")]", name));
    } else {
        new_struct.derive("ToSql");
        new_struct.derive("FromSql");
        new_struct.r#macro(&format!("#[postgres(name = \"{}\")]", name));
    }

    // Generate the struct fields
    // NOTE: for structs, there is no way to indicate which fields are NOT NULL, so we assume all of them are,
    //       otherwise we have to wrap everything in an Option
    let mut copyable = true;
    let mut serializable = true;
    for field in fields {
        let rs_name = gen_fld_name(&field.name);
        let foo = resolve_fld_type(database, field.typ)?;
        copyable = copyable & foo.copyable;
        serializable = serializable & foo.serializable;
        let rs_type = if field.is_nullable && (is_table || is_view) {
            format!("Option<{}>", foo.rs_type)
        } else {
            foo.rs_type
        };

        let mut fld = codegen::Field::new(&format!("pub {}", rs_name), rs_type);
        if !rs_name.eq(&field.name) {
            fld.annotation(vec![&format!("#[postgres(name = \"{}\")]", field.name)]);
        }
        new_struct.push_field(fld);
    }

    // Can be serialized?
    if copyable {
        new_struct.derive("Copy");
    }
    if serializable {
        new_struct.derive("Serialize");
        new_struct.derive("Deserialize");
    }

    // Done
    Ok(())
}

fn gen_function(database: &BTreeMap<Oid, PgType>, func_def: &PgFunction) -> Result<codegen::Function, String> {
    // Create the new function definition
    let mut new_func = codegen::Function::new(&gen_function_name(&func_def.schema, &func_def.name));

    // Make it public
    new_func.vis("pub");

    // Function arguments
    new_func.arg_ref_self();
    for arg in &func_def.arguments {
        let arg_type = resolve_arg_type(&database, arg.typ)?;
        new_func.arg(&gen_arg_name(&arg.name), arg_type);
    }

    // Function return type
    let foo = match &func_def.returns {
        PgReturn::Void => format!("()"),
        PgReturn::Scalar(typ) => resolve_ret_type(&database, *typ)?,
        PgReturn::Record(r) => {
            let foo = r
                .iter()
                .map(|field| resolve_ret_type(&database, field.typ))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            format!("({})", foo)
        }
    };
    let func_ret = if func_def.returns_set { format!("Vec<({})>", foo) } else { foo };
    new_func.ret(format!("PgResult<{}>", func_ret));

    // Function body
    // 1st part, SQL query
    let pg_args = (0..func_def.arguments.len())
        .map(|x| format!("${}", x + 1))
        .collect::<Vec<String>>()
        .join(", ");
    let rs_args = func_def
        .arguments
        .iter()
        .map(|arg| format!("&{}", &gen_arg_name(&arg.name)))
        .collect::<Vec<String>>()
        .join(", ");
    match (func_def.kind, &func_def.returns) {
        (PgProcedureKind::Function, PgReturn::Void) => {
            new_func.line(format!(
                "let _query = self.query(\"SELECT * FROM \\\"{}\\\".\\\"{}\\\"({})\", &[{}])?;",
                &func_def.schema, &func_def.name, &pg_args, &rs_args
            ));
        }
        (PgProcedureKind::Function, _) => {
            new_func.line(format!(
                "let query = self.query(\"SELECT * FROM \\\"{}\\\".\\\"{}\\\"({})\", &[{}])?;",
                &func_def.schema, &func_def.name, &pg_args, &rs_args
            ));
        }
        (PgProcedureKind::Proc, _) => {
            new_func.line(format!(
                "let _query = self.query(\"CALL \\\"{}\\\".\\\"{}\\\"({})\", &[{}])?;",
                &func_def.schema, &func_def.name, &pg_args, &rs_args
            ));
        }
        _ => panic!(
            "Unsupported kind of function {:#?}: \\\"{}\\\".\\\"{}\\\"",
            func_def.kind, func_def.schema, func_def.name
        ),
    }

    // Function body
    // 2nd part, result extraction
    match (&func_def.returns, func_def.returns_set) {
        // Void
        (PgReturn::Void, _) => {
            new_func.line("Ok(())");
        }
        // Returns a single scalar (may actually be a composite)
        (PgReturn::Scalar(typ), false) => {
            new_func.line("let row = query.into_iter().next()?;");
            match database.get(&typ).ok_or("Unknown return type")? {
                PgType::Composite { schema, name, .. } => {
                    new_func.line(format!("let result = {}::from_postgres_row(row)?;", gen_type_name(schema, name)));
                }
                _ => {
                    new_func.line("let result = row.get(0);");
                }
            }
            new_func.line("Ok(result)");
        }
        // Returns a set of scalars (may actually be a composite)
        (PgReturn::Scalar(typ), true) => match database.get(&typ).ok_or("Unknown return type")? {
            PgType::Composite { schema, name, .. } => {
                new_func.line(format!(
                    "let result: Result<Vec<{0}>, _> = query.iter().map(|row| {0}::from_postgres_row(row)).collect();",
                    gen_type_name(schema, name)
                ));
                new_func.line("Ok(result?)");
            }
            _ => {
                new_func.line("let result = query.into_iter().map(|row| row.get(0)).collect();");
                new_func.line("Ok(result)");
            }
        },
        // Returns a single record (anonymous composite)
        (PgReturn::Record(r), false) => {
            new_func.line(format!("let row = query.into_iter().next()?;"));
            let foo = r
                .iter()
                .map(|ret| format!("row.get(\\\"{}\\\")", ret.name))
                .collect::<Vec<String>>()
                .join(",");
            new_func.line(format!("let result = ({});", foo));
            new_func.line("Ok(result)");
        }
        // Returns a set of records (anonymous composites)
        (PgReturn::Record(r), true) => {
            let foo = r
                .iter()
                .map(|ret| format!("row.get(\\\"{}\\\")", ret.name))
                .collect::<Vec<String>>()
                .join(",");
            new_func.line(format!("let result = query.into_iter().map(|row| ({})).collect();", foo));
            new_func.line("Ok(result)");
        }
    }

    // Done
    Ok(new_func)
}

/// Convert a PostgreSQL type name to a safe Rust name
///
fn gen_type_name(schema: &String, name: &String) -> String {
    if schema == "public" {
        name.to_camel_case()
    } else {
        format!("{}_{}", schema.to_camel_case(), name.to_camel_case())
    }
}

/// Convert a PostgreSQL function name to a safe Rust name
///
fn gen_function_name(schema: &String, name: &String) -> String {
    if schema == "public" {
        name.to_snake_case()
    } else {
        format!("{}_{}", schema.to_snake_case(), name.to_snake_case())
    }
}

/// Convert a PostgreSQL field name to a safe Rust name
///
fn gen_fld_name(name: &String) -> String {
    // format!("pub {}", name.to_snake_case())
    name.to_snake_case()
}

/// Convert a PostgreSQL argument name to a safe Rust name
///
fn gen_arg_name(name: &String) -> String {
    name.to_snake_case()
}

/// Generate the Rust definition for a PostgreSQL type
///
fn resolve_fld_type(database: &BTreeMap<Oid, PgType>, oid: Oid) -> Result<TypeCorrespondence, String> {
    match database.get(&oid).ok_or(format!("Unknown type #{}", oid))? {
        PgType::Base(rust_type) => Ok(rust_type.clone()),
        PgType::Enum { schema, name, .. } => Ok(TypeCorrespondence {
            rs_type: gen_type_name(schema, name),
            copyable: true,
            serializable: true,
        }),
        PgType::Composite { schema, name, fields, .. } => {
            let copyable = fields.iter().all(|f| resolve_fld_type(database, f.typ).unwrap().copyable);
            let serializable = fields.iter().all(|f| resolve_fld_type(database, f.typ).unwrap().serializable);
            Ok(TypeCorrespondence {
                rs_type: gen_type_name(schema, name),
                copyable,
                serializable,
            })
        }
        PgType::Domain { base_type, .. } => resolve_fld_type(database, *base_type),
        PgType::Array { base_type, .. } => {
            let inner = resolve_fld_type(database, *base_type)?;
            Ok(TypeCorrespondence {
                rs_type: format!("Vec<{}>", inner.rs_type),
                copyable: false,
                serializable: inner.serializable,
            })
        }
        PgType::Range { base_type, .. } => {
            let inner = resolve_fld_type(database, *base_type)?;
            Ok(TypeCorrespondence {
                rs_type: format!("postgres_range::Range<{}>", inner.rs_type),
                copyable: inner.copyable,
                serializable: false,
            })
        }
        PgType::Unknown(name) => Err(format!("Unknown type #{} ({})", oid, name)),
    }
}

/// Generate the Rust definition for a function argument
///
fn resolve_arg_type(database: &BTreeMap<Oid, PgType>, oid: Oid) -> Result<String, String> {
    match database.get(&oid).ok_or(format!("Unknown type #{}", oid))? {
        PgType::Base(inner) => {
            if inner.copyable {
                Ok(inner.rs_type.clone())
            } else {
                Ok(format!("&{}", inner.rs_type))
            }
        }
        PgType::Enum { schema, name, .. } => Ok(gen_type_name(schema, name)),
        PgType::Composite { schema, name, .. } => {
            let inner = gen_type_name(schema, name);
            Ok(format!("&{}", inner))
        }
        PgType::Domain { base_type, .. } => {
            let inner = resolve_fld_type(database, *base_type)?;
            if inner.copyable {
                Ok(inner.rs_type.clone())
            } else {
                Ok(format!("&{}", inner.rs_type))
            }
        }
        PgType::Array { base_type, .. } => {
            let inner = resolve_arg_type(database, *base_type)?;
            Ok(format!("&[{}]", inner))
        }
        PgType::Range { base_type, .. } => {
            let inner = resolve_arg_type(database, *base_type)?;
            Ok(format!("&postgres_range::Range<{}>", inner))
        }
        PgType::Unknown(name) => Err(format!("Unknown type #{} ({})", oid, name)),
    }
}

/// Generate the Rust definition for a function result
///
fn resolve_ret_type(database: &BTreeMap<Oid, PgType>, oid: Oid) -> Result<String, String> {
    match database.get(&oid).ok_or(format!("Unknown type #{}", oid))? {
        PgType::Base(inner) => Ok(inner.rs_type.clone()),
        PgType::Enum { schema, name, .. } => Ok(gen_type_name(schema, name)),
        PgType::Composite { schema, name, .. } => Ok(gen_type_name(schema, name)),
        PgType::Domain { base_type, .. } => resolve_ret_type(database, *base_type),
        PgType::Array { base_type, .. } => {
            let inner = resolve_ret_type(database, *base_type)?;
            Ok(format!("Vec<{}>", inner))
        }
        PgType::Range { base_type, .. } => {
            let inner = resolve_ret_type(database, *base_type)?;
            Ok(format!("postgres_range::Range<{}>", inner))
        }
        PgType::Unknown(name) => Err(format!("Unknown type #{} ({})", oid, name)),
    }
}
