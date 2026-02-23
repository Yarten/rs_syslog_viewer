use crate::{
  app::{
    Controller, LogHub, StateBuilder,
    controller::{DebugController, LogController, TagController},
    page::{DebugPage, LogPage, TagPage, log_page},
    state::{DebugOperationState, LogNavigationState, TagOperationState},
  },
  debug,
  log::Config as LogConfig,
  ui::{
    KeyEventEx, Pager, State, StateMachine, pager::Theme as PagerTheme,
    state_machine::Config as SmConfig,
  },
};
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::DefaultTerminal;
use std::{
  collections::HashMap,
  path::PathBuf,
  {cell::RefCell, rc::Rc},
};

/// 程序配置
pub struct Config {
  /// 日志目录
  pub logs_root: PathBuf,

  /// 各个系统日志及其读取配置
  pub logs_configs: HashMap<String, LogConfig>,

  /// 页面整体的风格
  pub pager_theme: PagerTheme,

  /// 状态机的配置
  pub sm_config: SmConfig,

  /// 调试用的日志记录缓存大小
  pub debug_buffer_size: usize,

  /// 日志页面的渲染配置
  pub log_page_config: log_page::Config,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      logs_root: Default::default(),
      logs_configs: Default::default(),
      pager_theme: Default::default(),
      sm_config: Default::default(),
      debug_buffer_size: 200,
      log_page_config: Default::default(),
    }
  }
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

const TAG_PAGE: usize = 1;
const DEBUG_PAGE: usize = 2;

/// 辅助构建状态机的类
struct StateMachineBuilder {
  sm_config: SmConfig,
  log_nav_state: State,
  tag_nav_state: State,
  debug_nav_state: State,
}

impl StateMachineBuilder {
  fn build(self) -> StateMachine {
    const LOG_NAV_STATE: usize = 1;
    const TAG_NAV_STATE: usize = 2;
    const DEBUG_NAV_STATE: usize = 3;

    StateMachine::new(self.sm_config)
      // 根状态，也即日志导航状态
      .root_state(
        LOG_NAV_STATE,
        self
          .log_nav_state
          .enter_action(|pager| {
            pager.focus_root();
            pager.status().set_info("press 'h' for help");
          })
          // 按 t 或 ctrl+t 聚焦与开关标签过滤页面
          .goto_action(
            KeyEvent::simple(KeyCode::Char('t')),
            TAG_NAV_STATE,
            |pager| {
              pager.open_left(TAG_PAGE);
              true
            },
          )
          .action(KeyEvent::ctrl('t'), |pager| pager.toggle_left(TAG_PAGE))
          // 按 d 或 ctrl+d 聚焦与开关标签过滤页面
          .goto_action(
            KeyEvent::simple(KeyCode::Char('d')),
            DEBUG_NAV_STATE,
            |pager| {
              pager.open_right(DEBUG_PAGE);
              true
            },
          )
          .action(KeyEvent::ctrl('d'), |pager| pager.toggle_right(DEBUG_PAGE)),
      )
      // 标签导航状态
      .state(
        TAG_NAV_STATE,
        self
          .tag_nav_state
          .enter_action(|pager| pager.focus(TAG_PAGE))
          .goto(KeyEvent::simple(KeyCode::Esc), LOG_NAV_STATE),
      )
      // 调试界面状态
      .state(
        DEBUG_NAV_STATE,
        self
          .debug_nav_state
          .enter_action(|pager| {
            pager.focus(DEBUG_PAGE);
            pager.status().set_info("press 'd' or esc to unfocus");
          })
          .goto(KeyEvent::simple(KeyCode::Esc), LOG_NAV_STATE)
          .goto(KeyEvent::simple(KeyCode::Char('d')), LOG_NAV_STATE),
      )
  }
}

impl Viewer {
  /// 启动 UI 渲染流程，包装核心循环，并做好资源回收
  pub fn run(config: Config) -> Result<()> {
    color_eyre::install()?;
    debug::enable_debug(config.debug_buffer_size);
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
    let log_controller = Rc::new(RefCell::new(LogController::default()));
    let tag_controller = Rc::new(RefCell::new(TagController::default()));
    let debug_controller = Rc::new(RefCell::new(DebugController::default()));

    // ------------------------------------------
    // 记录所有控制器
    let controllers: Vec<Rc<RefCell<dyn Controller>>> = vec![
      log_controller.clone(),
      tag_controller.clone(),
      debug_controller.clone(),
    ];

    // ------------------------------------------
    // 构建状态机与状态
    let sm = StateMachineBuilder {
      sm_config: config.sm_config,
      log_nav_state: LogNavigationState::new(log_controller.clone()).build(),
      tag_nav_state: TagOperationState::new(tag_controller.clone()).build(),
      debug_nav_state: DebugOperationState::new(debug_controller.clone()).build(),
    }
    .build();

    // ------------------------------------------
    // 构建页面
    let pager = Pager::new(config.pager_theme)
      .add_page_as_root(LogPage {
        log_controller,
        config: config.log_page_config,
      })
      .add_page(TAG_PAGE, TagPage { tag_controller })
      .add_page(DEBUG_PAGE, DebugPage { debug_controller });

    // ------------------------------------------
    // 构造并返回本类对象
    Viewer {
      log_hub,
      pager,
      sm,
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
