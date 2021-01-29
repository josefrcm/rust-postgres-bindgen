use clap::Clap;
use std::{io::prelude::*, str::FromStr};

#[derive(Clap)]
#[clap(version = "1.0", author = "José Franco Campos <franco.jose@qst.go.jp>")]
struct Opts {
    /// PostgreSQL connection string,
    /// for details please refer to https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING
    #[clap(long, conflicts_with_all = &["host", "port", "user", "password", "dbname"])]
    url: Option<String>,
    /// PostgreSQL host name
    #[clap(short, long, default_value = "localhost")]
    host: String,
    /// PostgreSQL port
    #[clap(short, long, default_value = "5432")]
    port: u16,
    /// PostgreSQL user name
    #[clap(short, long, default_value = "postgres")]
    user: String,
    /// PostgreSQL password
    #[clap(short = 'w', long)]
    password: Option<String>,
    /// PostgreSQL database name
    #[clap(short, long, default_value = "postgres")]
    dbname: String,
    /// Ouput file, if no file is provided results will be written to stdout
    #[clap(short, long)]
    output_file: Option<std::path::PathBuf>,
}

fn main() -> std::io::Result<()> {
    // Parse the program options
    let opts: Opts = Opts::parse();

    // Read the PostgreSQL connection configuration
    let mut conn_config = postgres::config::Config::new();
    if let Some(url) = opts.url {
        conn_config = postgres::config::Config::from_str(&url).unwrap();
    } else {
        conn_config.host(&opts.host);
        conn_config.port(opts.port);
        conn_config.user(&opts.user);
        if let Some(password) = opts.password {
            conn_config.password(&password);
        }
        conn_config.dbname(&opts.dbname);
    }

    // Run the transformation
    let code = postgres_bindgen::run(&conn_config).to_string();

    // Write the result
    if let Some(path) = opts.output_file {
        // Create the output directory if it doesn't exist
        let output_dir = path.parent().unwrap_or(std::path::Path::new("."));
        std::fs::create_dir_all(output_dir).unwrap();

        // Write the result
        let mut file = std::fs::File::create(path)?;
        file.write_all(code.as_bytes())?;
    } else {
        println!("{}", code);
    }
    Ok(())
}
