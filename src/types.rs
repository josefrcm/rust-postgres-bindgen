pub use postgres::types::Oid;
pub use std::collections::BTreeMap;

// --------------------------------------------------------------------------------------------------------------------
// PostgreSQL data types
// --------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TypeCorrespondence {
    pub rs_type: String,
    pub copyable: bool,
    pub serializable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PgDatabase {
    pub types: BTreeMap<Oid, PgType>,
    pub functions: BTreeMap<Oid, PgFunction>,
}

// https://www.postgresql.org/docs/current/datatype-pseudo.html
// User defined types
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum PgType {
    Base(TypeCorrespondence),
    Enum { schema: String, name: String, values: Vec<String> },
    Composite { schema: String, name: String, is_table: bool, is_view: bool, fields: Vec<PgField> },
    Domain { schema: String, name: String, base_type: Oid },
    Array { schema: String, name: String, base_type: Oid },
    Range { schema: String, name: String, base_type: Oid },
    Unknown(String),
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum PgTypeKind {
    Base,
    Pseudo,
    Enum,
    Composite,
    Domain,
    Array,
    Range,
}

// Field of a table or a composite type
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PgField {
    pub name: String,
    //pos: u32,
    pub typ: Oid,
    pub is_nullable: bool,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PgFunction {
    pub schema: String,
    pub name: String,
    pub kind: PgProcedureKind,
    pub is_strict: bool,
    pub arguments: Vec<PgArgument>,
    pub returns: PgReturn,
    pub returns_set: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PgReturn {
    Void,
    Scalar(Oid),
    Record(Vec<PgArgument>),
}

// Argument of a function
#[derive(Debug, Serialize, Deserialize)]
pub struct PgArgument {
    pub name: String,
    pub typ: Oid,
    pub is_variadic: bool,
    pub is_nullable: bool,
}

/// Argument mode
#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone)]
pub enum PgArgumentMode {
    In,
    Out,
    InOut,
    Variadic,
    Table,
}

/// Kind of stored procedure
#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone)]
pub enum PgProcedureKind {
    Function,
    Proc,
    Aggregate,
    Window,
}
