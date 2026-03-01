use color_eyre::Result;
use rs_syslog_viewer::{
  app::{Config, Viewer},
  log::Config as LogConfig,
};
use std::path::PathBuf;

fn main() -> Result<()> {
  Viewer::run(Config {
    logs_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data2"),
    logs_configs: ('a'..'z')
      .map(|c| {
        (
          format!("{}{}{}{}", c.to_uppercase(), c, c, c),
          LogConfig::default(),
        )
      })
      .collect(),
    ..Config::default()
  })
}
