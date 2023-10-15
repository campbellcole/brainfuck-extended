use std::{
    fs,
    io::{Read, Write},
    process::{Command, Stdio},
};

use chrono::Utc;
use color_eyre::eyre::{eyre, Result};
use proc_macro2::TokenStream;

use crate::Cli;

const MANIFEST_TEMPLATE: &str = include_str!("./Cargo.toml.TEMPLATE");
const README_TEMPLATE: &str = include_str!("./README.md.TEMPLATE");

struct Replacements<'a> {
    package_name: &'a str,
    source_filename: &'a str,
    source_code: &'a str,
    timestamp: &'a str,
}

impl<'a> Replacements<'a> {
    pub fn run(&self, orig: &str) -> String {
        orig.replace("%%PACKAGE_NAME%%", &self.package_name)
            .replace("%%SOURCE_FILENAME%%", &self.source_filename)
            .replace("%%SOURCE_CODE%%", &self.source_code)
            .replace("%%TIMESTAMP%%", &self.timestamp)
    }
}

pub fn generate_crate_for_code(cli: &Cli, in_code: &str, out_code: TokenStream) -> Result<()> {
    fs::create_dir_all(&cli.output)?;

    let package_name = cli.output.file_stem().unwrap().to_str().unwrap();
    let source_filename = cli.input.file_name().unwrap().to_str().unwrap();
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let replacements = Replacements {
        package_name,
        source_filename,
        source_code: in_code,
        timestamp: &timestamp,
    };

    let manifest = replacements.run(MANIFEST_TEMPLATE);
    fs::write(cli.output.join("Cargo.toml"), manifest)?;

    let readme = replacements.run(README_TEMPLATE);
    fs::write(cli.output.join("README.md"), readme)?;

    fs::copy(&cli.input, cli.output.join(source_filename))?;

    fs::create_dir_all(cli.output.join("src"))?;

    if !cli.format {
        fs::write(cli.output.join("src").join("main.rs"), out_code.to_string())?;
    } else {
        let mut cmd = Command::new("rustfmt");
        cmd.arg("--emit=stdout");
        cmd.arg("--edition=2021");

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd.spawn()?;

        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();

        stdin.write_all(out_code.to_string().as_bytes())?;

        drop(stdin);

        let mut out = String::new();

        stdout.read_to_string(&mut out)?;

        drop(stdout);

        fs::write(cli.output.join("src").join("main.rs"), out)?;

        let status = child.wait()?;
        if !status.success() {
            return Err(eyre!("rustfmt failed"));
        }
    }

    Ok(())
}
