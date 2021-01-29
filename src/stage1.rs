use std::collections::BTreeMap;

use postgres::types::{accepts, FromSql, Type};
use std::error::Error;

use crate::types::*;

// --------------------------------------------------------------------------------------------------------------------
// Functions
// --------------------------------------------------------------------------------------------------------------------

/// Get all interesting definitions from the database
pub fn run(conn_config: &postgres::config::Config) -> PgDatabase {
    // Connect to the database
    let mut client = conn_config.connect(postgres::NoTls).unwrap();

    // Initialize the database with the system types
    let mut database = PgDatabase::new(&mut client);

    // Read the user types
    let types_sql = include_str!("resources/types.sql");
    for row in client.query(types_sql, &[]).unwrap() {
        match parse_type(row) {
            Ok((oid, typ)) => {
                database.types.insert(oid, typ);
            }
            Err(e) => {
                eprintln!("{}", e);
            }
        }
    }

    // Read the user functions and procedures
    let functions_sql = include_str!("resources/functions.sql");
    for row in client.query(functions_sql, &[]).unwrap() {
        match parse_function(row) {
            Ok((oid, func)) => {
                database.functions.insert(oid, func);
            }
            Err(e) => {
                eprintln!("{}", e);
            }
        }
    }

    // Done
    return database;
}

// --------------------------------------------------------------------------------------------------------------------
// Private stuff
// --------------------------------------------------------------------------------------------------------------------

impl PgDatabase {
    /// https://docs.rs/postgres-types/0.1.3/src/postgres_types/type_gen.rs.html
    /// Check how they generate the file, and do the same
    pub fn new(pg: &mut postgres::Client) -> Self {
        // Read the list of defined types
        let equivalences: BTreeMap<String, TypeCorrespondence> = ron::from_str(include_str!("resources/mapping.ron")).unwrap();

        // Get their OIDs from the database
        let mut types: BTreeMap<Oid, PgType> = BTreeMap::new();
        let catalog_sql = include_str!("resources/catalog.sql");
        for row in pg.query(catalog_sql, &[]).unwrap() {
            let oid: Oid = row.get("oid");
            let schema = "pg_catalog".to_string();
            let name: String = row.get("name");
            let kind: i8 = row.get("kind");
            let base_type: Oid = row.get("base_type");

            let typ = match std::char::from_u32(kind as u32).unwrap() {
                'b' | 'p' => match equivalences.get(&name) {
                    Some(n) => PgType::Base(n.clone()),
                    None => PgType::Unknown(name.clone()),
                },
                'd' => PgType::Domain { schema, name, base_type },
                'r' => PgType::Range { schema, name, base_type },
                'a' => PgType::Array { schema, name, base_type },
                //'e' => PgType::Enum { schema, name, base_type },
                e => panic!("Unknown type kind '{}'", e),
            };

            /*match postgres::types::Type::from_oid(oid) {
                None => {}
                Some(t) => {
                    println!("{}, {}, {}, {:#?}", oid, t.schema(), t.name(), t.kind());
                }
            }*/
            types.insert(oid, typ);
        }

        // Add the system catalog
        let functions = BTreeMap::new();
        Self { types, functions }
    }
}

/// Parse a type declaration
/// https://www.postgresql.org/docs/current/catalog-pg-type.html
///
fn parse_type(row: postgres::row::Row) -> Result<(Oid, PgType), String> {
    // Common fields for all types
    let oid = row.get("oid");
    let schema = row.get("schema");
    let name = row.get("name");
    //let description = row.get("description");
    let kind = row.get("kind");
    let is_table = row.get("is_table");
    let is_view = row.get("is_view");

    // Only one of these fields will be used, depending on the kind of type
    let enum_values = row.get("enum_values");
    let struct_fields = row.get("struct_fields");
    let base_type = row.get("base_type");

    match kind {
        PgTypeKind::Enum => Ok((
            oid,
            PgType::Enum {
                schema,
                name,
                values: enum_values,
            },
        )),
        PgTypeKind::Composite => Ok((
            oid,
            PgType::Composite {
                schema,
                name,
                is_table,
                is_view,
                fields: serde_json::from_value(struct_fields).unwrap(),
            },
        )),
        PgTypeKind::Domain => Ok((oid, PgType::Domain { schema, name, base_type })),
        PgTypeKind::Range => Ok((oid, PgType::Range { schema, name, base_type })),
        PgTypeKind::Array => Ok((oid, PgType::Array { schema, name, base_type })),
        PgTypeKind::Base => Err(format!("Base types shouldn't be here! {} -> {}.{}'", oid, schema, name)),
        PgTypeKind::Pseudo => Err(format!("Pseudo types shouldn't be here! {} -> {}.{}'", oid, schema, name)),
    }
}

/// Parse a function declaration
/// https://www.postgresql.org/docs/current/catalog-pg-proc.html
///
fn parse_function(row: postgres::row::Row) -> Result<(Oid, PgFunction), String> {
    // Base information
    let oid = row.get("oid");
    let schema = row.get("schema");
    let name = row.get("name");
    let kind = row.get("kind");
    let is_strict: bool = row.get("is_strict");

    // Ignore aggregate and window functions
    if kind == PgProcedureKind::Aggregate {
        return Err(format!("Aggregate functions are not supported: {} -> {}.{}", oid, schema, name));
    }
    if kind == PgProcedureKind::Window {
        return Err(format!("Window functions are not supported: {} -> {}.{}", oid, schema, name));
    }

    // Arguments are defined using three arrays
    //   - proargnames: the name of every argument
    //   - proargtypes: the data types of the function arguments, but only input arguments (including INOUT and VARIADIC arguments).
    //   - proallargtypes: the data types of all the function arguments (including OUT and INOUT arguments);
    //     however, if all the arguments are IN arguments, this field will be null.
    //   - proargmodes: modes of the function arguments, encoded as:
    //       - i for IN arguments
    //       - o for OUT arguments
    //       - b for INOUT arguments
    //       - v for VARIADIC arguments
    //       - t for TABLE arguments
    //     If all the arguments are IN arguments, this field will be null
    let arg_names: Vec<String> = match row.get("arg_names") {
        None => Vec::new(),
        Some(x) => x,
    };
    let arg_types: Vec<Oid> = match row.get("arg_types") {
        None => Vec::new(),
        Some(x) => x,
    };
    let arg_modes: Vec<PgArgumentMode> = match row.get("arg_modes") {
        None => vec![PgArgumentMode::In; arg_names.len()],
        Some(x) => x,
    };

    // Return type
    // Ignore triggers
    let ret_type: Oid = row.get("ret_type");
    let mut returns_set: bool = row.get("ret_set");
    if ret_type == postgres::types::Type::TRIGGER.oid() {
        return Err(format!("Trigger functions not supported: {} -> {}.{}", oid, schema, name));
    }

    // We need to iterate the three arrays in parallel to extract the arguments
    //  - TABLE arguments are the same as OUT, but the function also returns a set
    //  - VARIADIC arguments are IN, but go at the end of the list
    //  - FIXME: how do INOUT works
    let mut in_args = Vec::new();
    let mut out_args = Vec::new();
    let mut variadic_args = Vec::new();
    for (n, t, m) in izip!(arg_names, arg_types, arg_modes) {
        let mode = m;
        let is_variadic = mode == PgArgumentMode::Variadic;
        let foo = PgArgument {
            name: n,
            typ: t,
            is_variadic: is_variadic,
            is_nullable: !is_strict,
        };

        match mode {
            PgArgumentMode::In => in_args.push(foo),
            PgArgumentMode::Out => out_args.push(foo),
            PgArgumentMode::InOut => {
                return Err(format!("In/out arguments not supported yet: {} -> {}.{}", oid, schema, name));
            }
            PgArgumentMode::Variadic => variadic_args.push(foo),
            PgArgumentMode::Table => {
                out_args.push(foo);
                returns_set = true;
            }
        }
    }

    // Input arguments
    in_args.extend(variadic_args);
    let arguments = in_args;

    // Output arguments
    // If the output list if empty, then when need to add the return type
    // Otherwise the return type will be 'record' and it's unnecesary to handle it
    let returns = if kind == PgProcedureKind::Proc {
        PgReturn::Void
    } else if out_args.len() == 0 {
        if ret_type == postgres::types::Type::VOID.oid() {
            PgReturn::Void
        } else {
            PgReturn::Scalar(ret_type)
        }
    } else {
        PgReturn::Record(out_args)
    };

    // Done
    Ok((
        oid,
        PgFunction {
            schema,
            name,
            kind,
            is_strict,
            arguments,
            returns,
            returns_set,
        },
    ))
}

/// Parse the typtype field as comming from PostgreSQL
impl<'a> FromSql<'a> for PgTypeKind {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        let kind = postgres_protocol::types::char_from_sql(raw)?;
        match std::char::from_u32(kind as u32).unwrap() {
            'b' => Ok(PgTypeKind::Base),
            'p' => Ok(PgTypeKind::Pseudo),
            'd' => Ok(PgTypeKind::Domain),
            'r' => Ok(PgTypeKind::Range),
            'a' => Ok(PgTypeKind::Array),
            'e' => Ok(PgTypeKind::Enum),
            'c' => Ok(PgTypeKind::Composite),
            e => panic!("Unknown type kind '{}'", e),
        }
    }

    accepts!(CHAR);
}

/// Parse the prokind field as comming from PostgreSQL
/// TODO: make this an instance of FromSQL
impl<'a> FromSql<'a> for PgProcedureKind {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        let kind = postgres_protocol::types::char_from_sql(raw)?;
        match std::char::from_u32(kind as u32).unwrap() {
            'f' => Ok(PgProcedureKind::Function),
            'p' => Ok(PgProcedureKind::Proc),
            'a' => Ok(PgProcedureKind::Aggregate),
            'w' => Ok(PgProcedureKind::Window),
            e => panic!("Unknown procedure kind '{}'", e),
        }
    }

    accepts!(CHAR);
}

/// Parse the proargmode field as comming from PostgreSQL
/// TODO: make this an instance of FromSQL
impl<'a> FromSql<'a> for PgArgumentMode {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        let argmode = postgres_protocol::types::char_from_sql(raw)?;
        match std::char::from_u32(argmode as u32).unwrap() {
            'i' => Ok(PgArgumentMode::In),
            'o' => Ok(PgArgumentMode::Out),
            'b' => Ok(PgArgumentMode::InOut),
            'v' => Ok(PgArgumentMode::Variadic),
            't' => Ok(PgArgumentMode::Table),
            e => panic!("Unknown argument mode '{}'", e),
        }
    }

    accepts!(CHAR);
}
