#[macro_use]
extern crate itertools;

#[macro_use]
extern crate serde;

mod stage1;
mod stage2;
mod types;

// Run the transformation
pub fn run(conn_config: &postgres::config::Config) -> codegen::Scope {
    let pg_defs = stage1::run(&conn_config);
    let rs_defs = stage2::run(pg_defs);
    rs_defs
}
