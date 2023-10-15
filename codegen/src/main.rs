use std::{fs, path::PathBuf};

use ascii::AsciiString;
use clap::Parser;
use color_eyre::eyre::{Context, Result};
use generator::{BrainfuckToRust, CellSize, OverflowBehavior, PointerSafety};
use tracing_error::ErrorLayer;
use tracing_subscriber::{prelude::*, EnvFilter};

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate serde;

pub mod ast;
pub mod gen_crate;
pub mod generator;

// `Repeated` vectorizes repeated operations.
// Note that this does not improve performance
// in any way, it just makes the generated files
// significantly smaller.
pub type File = ast::File<ast::Repeated>;

// `Token` does no optimizations so the source
// code will be very large. The compiled binary
// is usually identical, byte for byte.
// pub type File = ast::File<ast::Token>;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    pub input: PathBuf,
    pub output: PathBuf,
    #[clap(short, long)]
    pub format: bool,
    #[clap(short, long)]
    pub dump_ast: Option<PathBuf>,
    #[clap(long)]
    pub fixed_input: Option<AsciiString>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_line_number(false)
                .with_file(true)
                .compact(),
        )
        .with(EnvFilter::from_default_env())
        .with(ErrorLayer::default())
        .init();

    color_eyre::install()?;

    let in_code = fs::read_to_string(&cli.input)?;

    let file: File = in_code.parse()?;

    if let Some(dump_ast) = &cli.dump_ast {
        fs::write(dump_ast, serde_json::to_string_pretty(&file)?)?;
    }

    let out_code = BrainfuckToRust::builder()
        .cell_size(CellSize::U8)
        .memory_size(30_000)
        .pointer_safety(PointerSafety::None)
        .overflow_behavior(OverflowBehavior::None)
        .fixed_input(cli.fixed_input.clone())
        .build()
        .generate(file)
        .wrap_err("failed to generate Rust from Brainfuck")?;

    gen_crate::generate_crate_for_code(&cli, &in_code, out_code)?;

    Ok(())
}
