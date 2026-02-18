use crate::{
  app::{Controller, LogController, LogHub, LogPage},
  log::Config as LogConfig,
  ui::{Pager, StateMachine, pager::Theme as PagerTheme},
};
use color_eyre::Result;
use ratatui::DefaultTerminal;
use std::collections::HashMap;
use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};

/// 程序配置
#[derive(Default)]
pub struct Config {
  /// 日志目录
  pub logs_root: PathBuf,

  /// 各个系统日志及其读取配置
  pub logs_configs: HashMap<String, LogConfig>,

  /// 页面整体的风格
  pub pager_theme: PagerTheme,
}

/// 日志可视化主体，也是该应用进程的启动入口
pub struct Viewer {
  /// 日志数据
  log_hub: LogHub,

  /// 页面管理器
  pager: Pager,

  /// 状态管理器
  sm: StateMachine,

  /// 所有的控制器
  controllers: Vec<Rc<RefCell<dyn Controller>>>,
}

impl Viewer {
  /// 启动 UI 渲染流程，包装核心循环，并做好资源回收
  pub fn run(config: Config) -> Result<()> {
    color_eyre::install()?;
    ratatui::run(|terminal| {
      // 创建 runtime
      let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create runtime");

      // 在 runtime 中运行异步代码
      rt.block_on(async {
        // 构建 viewer
        let mut viewer = Viewer::build(config);

        // 运行核心循环流程
        let res = viewer.main_loop(terminal).await;

        // 回收资源
        viewer.log_hub.close().await;

        // 返回核心流程的运行结果
        res
      })
    })
  }

  /// 构造可视化器
  fn build(config: Config) -> Self {
    // ------------------------------------------
    // 创建日志数据，此时文件已经在异步流程中读取了
    let log_hub = LogHub::open(config.logs_root, config.logs_configs);

    // ------------------------------------------
    // 创造各个控制器
    let log_controller = Rc::new(RefCell::new(LogController::new()));

    // ------------------------------------------
    // 记录所有控制器
    let controllers: Vec<Rc<RefCell<dyn Controller>>> = vec![log_controller.clone()];

    // ------------------------------------------
    // TODO: 构建状态

    // ------------------------------------------
    // 构建页面
    let pager = Pager::new(config.pager_theme).add_page_as_root(LogPage { log_controller });

    // ------------------------------------------
    // 构造并返回本类对象
    Viewer {
      log_hub,
      pager,
      sm: StateMachine::default(),
      controllers,
    }
  }

  /// 核心处理与渲染循环
  async fn main_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
    // 执行首次状态机的执行
    self.sm.first_run(&mut self.pager);

    // 数据处理与渲染循环
    loop {
      // 等待键盘事件，并响应它们。检查是否收到全局的退出信号，是则结束循环
      if self.sm.poll_once(&mut self.pager) {
        return Ok(());
      }

      {
        // 取出日志数据。此时，异步的读取流程会被停止
        let mut log_hub = self.log_hub.data().await;

        // 遍历所有控制器，进行数据处理与拷贝，并检查是否有控制器要求程序退出
        for controller in self.controllers.iter_mut() {
          let mut ctrl = controller.borrow_mut();
          ctrl.run_once(&mut log_hub);
          if ctrl.should_quit() {
            return Ok(());
          }
        }
      } // 日志数据处理结束，异步读取流程将自动运行。

      // 渲染页面，此时用的数据已经拷贝到各个控制器中
      terminal.draw(|frame| self.pager.render(frame))?;
    }
  }
}
