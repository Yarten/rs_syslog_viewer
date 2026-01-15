//! 监控文件变化，包括新内容增加、以及被重命名或删除

use crate::file::Event;
use anyhow::{Result, anyhow};
use notify::{
  Event as NotifyEvent, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher,
  event::{AccessKind, AccessMode},
};
use std::{
  path::{Path, PathBuf},
  time::Duration,
};
use tokio::{
  io::{self},
  sync::{mpsc, watch},
  task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Default)]
pub enum MetadataEvent {
  /// 其他未知的事件
  #[default]
  Any,

  /// 更名事件
  Renamed(PathBuf),

  /// 删除事件
  Removed,
}

impl MetadataEvent {
  /// 将自己转化为 reader::Event 发送出去，
  /// 返回是否中断轮询
  pub async fn send(self, tx: &mpsc::Sender<Event>) -> bool {
    match self {
      MetadataEvent::Any => false,
      MetadataEvent::Renamed(new_path) => {
        if let Err(e) = tx.send(Event::Renamed(new_path)).await {
          eprintln!("Failed to send renamed event: {e}");
          true
        } else {
          false
        }
      }
      MetadataEvent::Removed => {
        if let Err(e) = tx.send(Event::Removed).await {
          eprintln!("Failed to send removed event: {e}");
        }
        true
      }
    }
  }
}

pub enum ChangedEvent {
  Metadata(MetadataEvent),
  Content,
}

pub struct Watcher {
  /// 文件的 fd 路径，我们监控这个路径，无论文件名如何重命名，它都不变
  fd_path: PathBuf,

  /// 文件的原始名称，如果它和 fd_path 指向的路径不同，说明文件被重命名
  /// 如果 fd_path 指向的路径不存在，说明文件被删除
  raw_path: PathBuf,

  /// 内容变化通知通道
  content_event_rx: watch::Receiver<Result<NotifyEvent, notify::Error>>,

  /// 文件基础属性（如名称）的变化通知通道
  metadata_event_tx: watch::Sender<MetadataEvent>,
  metadata_event_rx: watch::Receiver<MetadataEvent>,

  /// 检查文件的轮询间隔
  poll_interval: Duration,

  /// 文件内容监控器
  content_watcher: RecommendedWatcher,

  /// 用于标识要取消监听的 cancel token
  cancel_token: CancellationToken,

  /// 轮询文件路径是否变化的 jh
  jh_watching_metadata: Option<JoinHandle<()>>,
}

impl Watcher {
  pub fn new(raw_path: &Path, fd_path: &Path, poll_interval: Duration) -> Result<Self> {
    // 创建内容监听器
    let (content_event_tx, content_event_rx) = watch::channel(Ok(NotifyEvent::default()));
    let watcher: RecommendedWatcher = notify::Watcher::new(
      move |res: notify::Result<NotifyEvent>| {
        let _ = content_event_tx.send(res);
      },
      notify::Config::default()
        .with_poll_interval(poll_interval)
        .with_compare_contents(false),
    )?;

    let (metadata_event_tx, metadata_event_rx) = watch::channel(MetadataEvent::default());

    // 创建本监控器
    Ok(Self {
      fd_path: fd_path.into(),
      raw_path: raw_path.into(),
      content_event_rx,
      metadata_event_tx,
      metadata_event_rx,
      poll_interval,
      content_watcher: watcher,
      cancel_token: CancellationToken::new(),
      jh_watching_metadata: None,
    })
  }

  pub fn start(&mut self) -> Result<()> {
    // 开始监控文件内容的变化
    self
      .content_watcher
      .watch(&self.fd_path, RecursiveMode::NonRecursive)?;

    // 开始监控文件路径的变化
    self.jh_watching_metadata = Some(self.spawn_watching_path_changed());

    Ok(())
  }

  pub async fn stop(&mut self) -> Result<()> {
    self.content_watcher.unwatch(&self.fd_path)?;

    self.cancel_token.cancel();
    if let Some(jh) = &mut self.jh_watching_metadata {
      jh.await?;
    }

    Ok(())
  }

  pub async fn changed(&mut self) -> Result<ChangedEvent> {
    loop {
      tokio::select! {
          // 监控文件内容的变化，来自 notify
          res = self.content_event_rx.changed() => {
              res?;

              let event;
              match &*self.content_event_rx.borrow_and_update() {
                  Ok(event_ref) => {
                      event = event_ref.clone();
                  },
                  Err(e) => {
                      eprintln!("Error when reading notify event {} of file {}", e, self.raw_path.to_str().unwrap_or(""));
                      return Err(anyhow!(e.to_string()));
                  }
              }

              if let EventKind::Modify(_) | EventKind::Access(AccessKind::Close(AccessMode::Write)) = event.kind {
                  return Ok(ChangedEvent::Content);
              } else {
                  continue;
              }

          },

          // 监控路径名称的变化，来自本类的异步轮询流程
          res = self.metadata_event_rx.changed() => {
              res?;
              match &*self.metadata_event_rx.borrow_and_update() {
                  MetadataEvent::Any => {
                      continue;
                  },
                  MetadataEvent::Renamed(new_path) => {
                      return Ok(ChangedEvent::Metadata(MetadataEvent::Renamed(new_path.clone())));
                  },
                  MetadataEvent::Removed => {
                      return Ok(ChangedEvent::Metadata(MetadataEvent::Removed));
                  }
              }
          }
      }
    }
  }

  fn spawn_watching_path_changed(&self) -> JoinHandle<()> {
    let tx = self.metadata_event_tx.clone();
    let cancel_token = self.cancel_token.clone();
    let poll_interval = self.poll_interval;
    let fd_path = self.fd_path.clone();
    let mut raw_path = self.raw_path.clone();

    tokio::spawn(async move {
      loop {
        tokio::select! {
            _ = cancel_token.cancelled() => break,
            _ = tokio::time::sleep(poll_interval) => {
                match fd_path.read_link() {
                    Ok(link) => {
                        // 轮询检查 fd 路径指向的真实路径内容是否发生变化
                        if link == raw_path {
                            continue;
                        }

                        // 检查路径末尾是否有被删除的标记，有说明文件被删除，发送删除事件并结束轮询
                        if link.ends_with("(deleted)") {
                            let _ = tx.send(MetadataEvent::Removed);
                            break;
                        }

                        // 名称如果变化，发送重命名事件，并等待进行下一次轮询
                        raw_path = link;
                        let _ = tx.send(MetadataEvent::Renamed(raw_path.clone()));
                    },
                    Err(e) => {
                        // 如果报错，我们也认为该文件被删除，发送删除事件并结束轮询
                        eprintln!("{} read link failed: {}", fd_path.to_str().unwrap_or(""), e);
                        let _ = tx.send(MetadataEvent::Removed);
                        break;
                    }
                }
            },
        }
      }
    })
  }
}
