use chrono::{DateTime, Duration, Local, SecondsFormat};
use glob::glob;
use itertools::Itertools;
use rand::distr::Alphanumeric;
use rand::prelude::*;
use std::io::Write;
use std::path::PathBuf;
use std::{fs, io};

struct Config {
  files_count: usize,
  line_length_range: (usize, usize),
  file_max_size_mb: usize,
  tags: Vec<(usize, String)>,
}

struct TimestampGenerator {
  tp: DateTime<Local>,

  // 连续日志倒计数，模拟日志短期爆发
  continual_tps: i32,

  // 上一次生成日志时的时间间隔
  dt_us: u64,
}

impl TimestampGenerator {
  fn new(tp: DateTime<Local>) -> TimestampGenerator {
    TimestampGenerator {
      tp,
      continual_tps: 0,
      dt_us: 0,
    }
  }

  fn next(&mut self) -> String {
    let res = self.tp.to_rfc3339_opts(SecondsFormat::Micros, false);

    let dt = if self.continual_tps <= 0 {
      // 生成间隔
      self.dt_us = rand::random_range(1..1000_000_000);

      // 是否连续生成，仅足够小的间隔这么生成
      if self.dt_us <= 10_000_000 && rand::random_bool(0.1) {
        self.continual_tps = rand::random_range(1..4096);
      }

      self.dt_us
    } else {
      // 连续生成间隔类似的日志
      self.continual_tps -= 1;

      // 在之前的间隔附近生成下一个间隔
      let half_dt_us = self.dt_us / 2;
      rand::random_range((self.dt_us - half_dt_us).max(1)..self.dt_us + half_dt_us)
    };

    // 更新下一个时间戳
    self.tp += Duration::microseconds(dt as i64);

    res
  }
}

fn main() {
  let mut rng = rand::rng();

  let mut tags = vec![
    "accounts-daemon",
    "acpid",
    "alsa-restore",
    "alsa-state",
    "anacron",
    "apparmor",
    "apport-autoreport",
    "apport",
    "apt-daily-upgrade",
    "apt-daily",
    "auditd",
    "auto-cpufreq",
    "avahi-daemon",
    "bluetooth",
    "cloud-init-local",
    "colord",
    "connman",
    "console-screen",
    "console-setup",
    "containerd",
    "corplink",
    "cron",
    "cups-browsed",
    "cups",
    "dbus",
    "dmesg",
    "docker",
    "dpkg-db-backup",
    "e2scrub_all",
    "e2scrub_reap",
    "emergency",
    "emqx",
    "firewalld",
    "fstrim",
    "fwupd-refresh",
    "fwupd",
    "gdm",
    "getty-static",
    "getty@tty1",
    "gpu-manager",
    "grub-common",
    "grub-initrd-fallback",
    "hv-fcopy-daemon",
    "hv-kvp-daemon",
    "hv-vss-daemon",
    "irqbalance",
    "kbd",
    "kerneloops",
    "keyboard-setup",
    "kmod-static-nodes",
    "logrotate",
    "man-db",
    "ModemManager",
    "modprobe@configfs",
    "modprobe@drm",
    "modprobe@efi_pstore",
    "modprobe@fuse",
    "motd-news",
    "netplan-ovs-cleanup",
    "networkd-dispatcher",
    "NetworkManager-wait-online",
    "NetworkManager",
    "nslcd",
    "nvidia-persistenced",
    "oem-config",
    "openvpn",
    "ovsdb-server",
    "packagekit",
    "plymouth-quit-wait",
    "plymouth-quit",
    "plymouth-read-write",
    "plymouth-start",
    "polkit",
    "power-profiles-daemon",
    "rc-local",
    "rescue",
    "rsyslog",
    "rtkit-daemon",
    "runsunloginclient",
    "secureboot-db",
    "setvtrgb",
    "snapd.aa-prompt-listener",
    "snapd.apparmor",
    "snapd.autoimport",
    "snapd.core-fixup",
    "snapd.failure",
    "snapd.recovery-chooser-trigger",
    "snapd.seeded",
    "snapd",
    "snapd.snap-repair",
    "ssh",
    "switcheroo-control",
    "system76-power",
    "systemd-ask-password-console",
    "systemd-ask-password-plymouth",
    "systemd-ask-password-wall",
    "systemd-backlight@backlight:nvidia_0.se",
    "systemd-backlight@leds:dell::kbd_backli",
    "systemd-binfmt",
    "systemd-boot-system-token",
    "systemd-fsck-root",
    "systemd-fsck@dev-disk-by\\x2duuid-1AD3\\x",
    "systemd-fsckd",
    "systemd-hwdb-update",
    "systemd-initctl",
    "systemd-journal-flush",
    "systemd-journald",
    "systemd-logind",
    "systemd-machine-id-commit",
    "systemd-modules-load",
    "systemd-networkd",
    "systemd-oomd",
    "systemd-pstore",
    "systemd-quotacheck",
    "systemd-random-seed",
    "systemd-remount-fs",
    "systemd-rfkill",
    "systemd-sysctl",
    "systemd-sysusers",
    "systemd-timesyncd",
    "systemd-tmpfiles-clean",
    "systemd-tmpfiles-setup-dev",
    "systemd-tmpfiles-setup",
    "systemd-udev-trigger",
    "systemd-udevd",
    "systemd-update-done",
    "systemd-update-utmp-runlevel",
    "systemd-update-utmp",
    "systemd-user-sessions",
    "systemd-vconsole-setup",
    "thermald",
    "todeskd",
    "tuned",
    "ua-auto-attach",
    "ua-reboot-cmds",
    "ua-timer",
    "ubuntu-advantage-cloud-id-shim",
    "ubuntu-advantage",
    "udisks2",
    "ufw",
    "unattended-upgrades",
    "update-notifier-download",
    "update-notifier-motd",
    "upower",
    "user-runtime-dir@1000",
    "user@1000",
    "uuidd",
    "walinuxagent",
    "whoopsie",
    "wpa_supplicant",
    "zfs-mount",
  ];
  let mut tags = tags
    .into_iter()
    .enumerate()
    .map(|(i, s)| (i + 1, s.to_string()))
    .collect_vec();
  tags.shuffle(&mut rng);

  let mut configs = vec![
    Config {
      files_count: 1,
      line_length_range: (1, 2048),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[0..1].to_vec(),
    },
    Config {
      files_count: 1,
      line_length_range: (1, 2048),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[1..3].to_vec(),
    },
    Config {
      files_count: 1,
      line_length_range: (1, 2048),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[3..6].to_vec(),
    },
    Config {
      files_count: 1,
      line_length_range: (256, 2048),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[6..10].to_vec(),
    },
    Config {
      files_count: 1,
      line_length_range: (256, 2048),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[10..14].to_vec(),
    },
    Config {
      files_count: 10,
      line_length_range: (512, 1024),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[14..16].to_vec(),
    },
    Config {
      files_count: 10,
      line_length_range: (512, 1024),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[16..18].to_vec(),
    },
    Config {
      files_count: 10,
      line_length_range: (512, 2048),
      file_max_size_mb: 100 * 1024 * 1024,
      tags: tags[18..33].to_vec(),
    },
    Config {
      files_count: 10,
      line_length_range: (512, 2048),
      file_max_size_mb: 100 * 1024 * 1024,
      tags: tags[33..58].to_vec(),
    },
    Config {
      files_count: 10,
      line_length_range: (512, 2048),
      file_max_size_mb: 100 * 1024 * 1024,
      tags: tags[58..60].to_vec(),
    },
    Config {
      files_count: 10,
      line_length_range: (512, 2048),
      file_max_size_mb: 100 * 1024 * 1024,
      tags: tags[60..63].to_vec(),
    },
    Config {
      files_count: 20,
      line_length_range: (512, 2048),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[63..65].to_vec(),
    },
    Config {
      files_count: 20,
      line_length_range: (512, 2048),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[65..75].to_vec(),
    },
    Config {
      files_count: 20,
      line_length_range: (512, 2048),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[75..85].to_vec(),
    },
    Config {
      files_count: 20,
      line_length_range: (512, 2048),
      file_max_size_mb: 10 * 1024 * 1024,
      tags: tags[85..86].to_vec(),
    },
    Config {
      files_count: 10,
      line_length_range: (512, 2048),
      file_max_size_mb: 200 * 1024 * 1024,
      tags: tags[86..100].to_vec(),
    },
    Config {
      files_count: 10,
      line_length_range: (512, 2048),
      file_max_size_mb: 200 * 1024 * 1024,
      tags: tags[100..120].to_vec(),
    },
    Config {
      files_count: 10,
      line_length_range: (512, 2048),
      file_max_size_mb: 200 * 1024 * 1024,
      tags: tags[120..].to_vec(),
    },
  ];

  // 对上述配置进行洗牌，不要
  configs.shuffle(&mut rand::rng());

  // 删除已有的 data2 下所有 log
  let root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data2/");
  for entry in glob(&format!("{}/*.log*", root_path.to_string_lossy())).unwrap() {
    let path = entry.unwrap();
    if path.is_file() {
      fs::remove_file(path).unwrap();
    }
  }

  // 日志时间从过去往现在
  let begin_tp = Local::now() - Duration::days(30);

  // 根据配置生成日志
  for (idx, c) in ('a'..='z').enumerate() {
    if idx >= configs.len() {
      break;
    }

    let file_name = format!("{}{}{}{}.log", c.to_uppercase(), c, c, c);
    let config = &configs[idx];
    let max_lines_count =
      config.file_max_size_mb * 2 / (config.line_length_range.0 + config.line_length_range.1);
    let mut tp = TimestampGenerator::new(begin_tp);

    for i in 0..config.files_count {
      // 第零份日志（也即最新的那一份），行数随机至最大行数；其余日志按最大行数生成。（也即被滚动的那些）
      let lines_count = if i == 0 {
        rand::random_range(1..max_lines_count)
      } else {
        max_lines_count
      };

      // 文件路径
      let file_path = if i == 0 {
        format!("{}/{}", root_path.to_string_lossy(), file_name)
      } else {
        format!("{}/{}.{}", root_path.to_string_lossy(), file_name, i)
      };

      // 打开文件
      let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .write(true)
        .open(file_path)
        .unwrap();
      let mut file = io::BufWriter::new(file);

      // 逐行生成日志
      for _ in 0..lines_count {
        let tag = config.tags.choose(&mut rng).unwrap();
        writeln!(
          &mut file,
          "{} yarten-Dell-G16-7630 {}[{}]: {}",
          tp.next(),
          tag.1,
          tag.0.to_string(),
          rand::rng()
            .sample_iter(&Alphanumeric)
            .take(rand::rng().random_range(config.line_length_range.0..config.line_length_range.1))
            .map(char::from)
            .collect::<String>()
        )
        .expect("Failed to write log");
      }
    }
  }
}
