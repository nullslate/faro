use crate::services::execute_replay;
use anyhow::bail;
use std::path::PathBuf;

use super::super::output::{output_status_text, print_json};
use super::super::store::open_store;

pub(crate) fn handle_replay(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let mut json_output = false;
    let mut positional = Vec::new();
    for arg in args {
        if arg == "--json" {
            json_output = true;
        } else {
            positional.push(arg);
        }
    }
    let Some(request_id) = positional.first().cloned() else {
        bail!("usage: faro replay <request-id> [--json]");
    };
    if positional.len() > 1 {
        bail!("usage: faro replay <request-id> [--json]");
    }

    let store = open_store(db_path)?;
    let result = execute_replay(&store, &request_id)?;
    if json_output {
        print_json(&result)?;
    } else {
        println!(
            "replay {} exit={} status={}",
            result.replay.id,
            output_status_text(result.replay.exit_code),
            output_status_text(result.replay.status_code)
        );
        if !result.stderr.is_empty() {
            eprintln!("{}", result.stderr);
        }
        print!("{}", result.stdout);
    }
    Ok(())
}
