use clap::Parser;
use color_eyre::Result;
use rs_syslog_viewer::{
  app::{Config, Viewer},
  log::Config as LogConfig,
};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// syslog viewer configured by command line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
  /// logs' root
  root: PathBuf,

  /// logs' names (without postfix)
  names: Vec<String>,
}

fn main() -> Result<()> {
  let args = Args::parse();

  Viewer::run(Config {
    logs_root: args.root,
    logs_configs: args
      .names
      .into_iter()
      .collect::<BTreeSet<String>>()
      .into_iter()
      .map(|s| (s, LogConfig::default()))
      .collect(),
    ..Default::default()
  })
}
