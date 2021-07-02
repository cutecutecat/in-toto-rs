//! A tool that functionaries can use to create link metadata about a step.

use std::collections::BTreeMap;
use std::fs::{metadata, File};
use std::io::{self, BufReader, Write};
use std::process::Command;
use walkdir::WalkDir;

use crate::models::{Link, TargetDescription};
use crate::{
    crypto,
    crypto::PrivateKey,
    models::{LinkMetadataBuilder, VirtualTargetPath},
};
use crate::{Error, Result};

/// record_artifacts is a function that traverses through the passed slice of paths, hashes the content of files
/// encountered, and returns the path and hashed content in BTreeMap format, wrapped in Result.
/// If a step in record_artifact fails, the error is returned.
pub fn record_artifacts(
    paths: &[&str],
    // hash_algorithms: Option<&[&str]>,
) -> Result<BTreeMap<VirtualTargetPath, TargetDescription>> {
    // Initialize artifacts
    let mut artifacts: BTreeMap<VirtualTargetPath, TargetDescription> = BTreeMap::new();

    // For each path provided, walk the directory and add all files to artifacts
    for path in paths {
        for entry in WalkDir::new(path) {
            let entry = match entry {
                Ok(content) => content,
                Err(error) => {
                    return Err(Error::from(io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Walkdir Error: {}", error),
                    )))
                }
            };
            let entry_path = entry.path();

            // TODO: Handle soft/symbolic links, by default is they are ignored, but we should visit them just once
            // TODO: Handle hidden files & directories

            // If entry is a file, open and hash the file
            let md = metadata(entry_path)?;
            if md.is_file() {
                let file = File::open(entry_path)?;
                let mut reader = BufReader::new(file);
                // TODO: handle optional hash_algorithms input
                let (_length, hashes) =
                    crypto::calculate_hashes(&mut reader, &[crypto::HashAlgorithm::Sha256])?;
                let path = entry_path.to_str().unwrap().to_string().replace("./", "");
                artifacts.insert(VirtualTargetPath::new(path)?, hashes);
            }
        }
    }
    Ok(artifacts)
}

/// run_command is a function that, given command arguments, executes commands on a software supply chain step
/// and returns the stdout and stderr as byproducts.
/// The first element of cmd_args is used as executable and the rest as command arguments.
/// If a commands in run_command fails to execute, the error is returned.
pub fn run_command(
    cmd_args: &[&str],
    // TODO run_dir: Option<&str>
) -> Result<BTreeMap<String, String>> {
    let mut cmd = Command::new(cmd_args[0]);
    let output = cmd.args(&cmd_args[1..]).output()?;

    // Emit stdout, stderror
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;

    // Format output into Byproduct
    let mut byproducts: BTreeMap<String, String> = BTreeMap::new();
    // Write to byproducts
    let stdout = match String::from_utf8(output.stdout) {
        Ok(output) => output,
        Err(error) => {
            return Err(Error::from(io::Error::new(
                std::io::ErrorKind::Other,
                format!("Utf8Error: {}", error),
            )))
        }
    };
    let stderr = match String::from_utf8(output.stderr) {
        Ok(output) => output,
        Err(error) => {
            return Err(Error::from(io::Error::new(
                std::io::ErrorKind::Other,
                format!("Utf8Error: {}", error),
            )))
        }
    };
    let status = match output.status.code() {
        Some(code) => code.to_string(),
        None => "Process terminated by signal".to_string()
    };

    byproducts.insert("stdout".to_string(), stdout);
    byproducts.insert("stderr".to_string(), stderr);
    byproducts.insert("return-value".to_string(), status);

    Ok(byproducts)
}

/// in_toto_run is a function that executes commands on a software supply chain step
/// (layout inspection coming soon), then generates and returns its corresponding Link metadata.
pub fn in_toto_run(
    name: &str,
    // run_dir: Option<&str>,
    material_paths: &[&str],
    product_paths: &[&str],
    cmd_args: &[&str],
    key: Option<PrivateKey>,
    // env: Option<BTreeMap<String, String>>
    // hash_algorithms: Option<&[&str]>,
) -> Result<Link> {
    // Record Materials: Given the material_paths, recursively traverse and record files in given path(s)
    let materials = record_artifacts(material_paths)?;

    // Execute commands provided in cmd_args
    let byproducts = run_command(cmd_args)?;

    // Record Products: Given the product_paths, recursively traverse and record files in given path(s)
    let products = record_artifacts(product_paths)?;

    // Create link based on values collected above
    let link_metadata_builder = LinkMetadataBuilder::new()
        .name(name.to_string())
        .materials(materials)
        .byproducts(byproducts)
        .products(products);
    let link_metadata = link_metadata_builder.build()?;

    // TODO Sign the link with key param supplied. If no key param supplied, build & return link
    /* match key {
        Some(k)   => {
            // TODO: SignedMetadata and Link are different types. Need to consolidate
            let signed_link = link_metadata_builder.signed::<Json>(&k).unwrap();
            let json = serde_json::to_value(&signed_link).unwrap();
        },
        None => {
        }
    } */
    Link::from(&link_metadata)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_record_artifacts() {
        assert_eq!(record_artifacts(&["tests"]).is_ok(), true);
        assert_eq!(record_artifacts(&["file-does-not-exist"]).is_err(), true);
    }

    #[test]
    fn test_run_command() {
        let byproducts = run_command(&["sh", "-c", "printf hello"]).unwrap();
        let mut expected = BTreeMap::new();
        expected.insert("stdout".to_string(), "hello".to_string());
        expected.insert("stderr".to_string(), "".to_string());
        expected.insert("return-value".to_string(), "1".to_string());

        assert_eq!(byproducts, expected);

        assert_eq!(
            run_command(&["command-does-not-exist", "true"]).is_err(),
            true
        );
    }
}
