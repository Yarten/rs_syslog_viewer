use color_eyre::Result;
use rs_syslog_viewer::{
  app::{Config, Viewer},
  log::Config as LogConfig,
};
use std::path::PathBuf;

fn main() -> Result<()> {
  Viewer::run(Config {
    logs_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data"),
    logs_configs: ["test", "user"]
      .into_iter()
      .map(|name| (name.to_string(), LogConfig::default()))
      .collect(),
    ..Config::default()
  })
}
