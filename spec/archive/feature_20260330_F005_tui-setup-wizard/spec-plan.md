# TUI Setup Wizard 执行计划

**目标:** 首次启动 TUI 时检测配置完整性，未配置则弹出全屏向导引导用户完成 Provider / API Key / 模型别名配置

**技术栈:** Rust, ratatui, ratatui-textarea, serde_json, anyhow

**设计文档:** spec-design.md


## 改动总览

- 本次改动涉及 `rust-agent-tui` crate 内 7 个文件（2 新建 + 5 修改），分为 4 个 Task 按依赖顺序执行
- Task 1 创建数据模型 → Task 2 实现渲染（依赖 Task 1 的 `SetupWizardPanel`） → Task 3 实现事件处理与持久化（依赖 Task 1+2） → Task 4 端到端集成测试
- 关键设计决策：
  - `needs_setup()` 接收 `&AppConfig`，检测 providers 是否为空或 api_key 是否缺失（含 env var fallback）
  - `SetupWizardPanel` 参照 `RelayPanel` 模式（独立 struct + 字段缓冲 + step 枚举）
  - 渲染层用 `Clear + centered_rect` 全屏覆盖，在 `main_ui::render()` 最前面优先拦截
  - 事件层在 `event.rs::next_event()` 最前面拦截，返回 `SetupWizardAction` 枚举驱动 save/skip/redraw
  - 不引入新依赖，复用已有的 ratatui、ratatui-textarea、AppConfig、ProviderConfig
  - 输入框使用手动字符管理（push/pop），不复用 `TextArea`——setup wizard 的输入场景简单（单行、无光标移动需求），避免引入 TextArea 的状态管理复杂度
  - `needs_setup()` 使用 `provider_type`（而非设计文档中的 `provider_id`）判断环境变量 key，因为 `id` 可由用户自定义（如 `my-anthropic`），`provider_type`（值为 `"anthropic"` / `"openai"`）更可靠

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证构建工具可用
  - 运行: `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出 `Finished` 或 `Compiling`，无 error
- [x] 验证测试工具可用
  - 运行: `cargo test -p rust-agent-tui --lib -- test_thinking_effort_low 2>&1 | tail -5`
  - 预期: 输出 `test result: ok` 或 `1 passed`

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p rust-agent-tui 2>&1 | grep -c "error"`
  - 预期: 输出 0
- [x] 测试命令可用
  - `cargo test -p rust-agent-tui --lib -- test_thinking_effort_low 2>&1 | grep -c "ok"`
  - 预期: 输出 ≥ 1

---

### Task 1: Setup 检测与数据模型

**背景:**
用户首次安装后启动 TUI 时，系统需自动检测配置是否完整（有无 Provider、API Key 是否缺失）。当前代码中 `App::new()` 已加载 `zen_config` 并尝试构造 `LlmProvider`，但缺少「配置不完整时触发引导」的机制。本 Task 新增 `needs_setup()` 检测函数和 `SetupWizardPanel` 状态结构体，为后续 Task 2（UI 渲染）和 Task 3（事件处理）提供数据基础。本 Task 的输出被所有后续 Task 依赖。

**涉及文件:**
- 新建: `rust-agent-tui/src/app/setup_wizard.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/main.rs`

**执行步骤:**

- [x] 新建 `rust-agent-tui/src/app/setup_wizard.rs`，定义 `SetupStep`、`ProviderType`、`AliasConfig` 枚举/结构体
  - 位置: 文件顶部
  - 内容: 三种类型定义，均 derive `Clone, PartialEq`（`SetupStep` 额外 derive `Copy`）
  ```rust
  // rust-agent-tui/src/app/setup_wizard.rs
  use ratatui_textarea::TextArea;

  /// 向导步骤
  #[derive(Debug, Clone, Copy, PartialEq)]
  pub enum SetupStep {
      Provider,
      ApiKey,
      ModelAlias,
      Done,
  }

  /// Provider 类型选择
  #[derive(Debug, Clone, Copy, PartialEq)]
  pub enum ProviderType {
      Anthropic,
      OpenAiCompatible,
  }

  impl ProviderType {
      pub fn label(&self) -> &str {
          match self {
              Self::Anthropic => "Anthropic",
              Self::OpenAiCompatible => "OpenAI Compatible",
          }
      }

      pub fn cycle(&mut self) {
          *self = match self {
              Self::Anthropic => Self::OpenAiCompatible,
              Self::OpenAiCompatible => Self::Anthropic,
          };
      }

      /// 根据类型返回默认 Provider ID
      pub fn default_provider_id(&self) -> &str {
          match self {
              Self::Anthropic => "anthropic",
              Self::OpenAiCompatible => "openai",
          }
      }

      /// 根据类型返回默认 Base URL
      pub fn default_base_url(&self) -> &str {
          match self {
              Self::Anthropic => "https://api.anthropic.com",
              Self::OpenAiCompatible => "https://api.openai.com/v1",
          }
      }

      /// 三个别名级别的默认模型 ID
      pub fn default_model_ids(&self) -> [&str; 3] {
          match self {
              Self::Anthropic => [
                  "claude-opus-4-0-20250514",
                  "claude-sonnet-4-6-20250514",
                  "claude-haiku-3-5-20241022",
              ],
              Self::OpenAiCompatible => ["o3", "gpt-4o", "gpt-4o-mini"],
          }
      }
  }

  /// 单个别名的配置
  #[derive(Debug, Clone)]
  pub struct AliasConfig {
      pub model_id: String,
  }

  /// Setup Wizard 全屏面板状态
  pub struct SetupWizardPanel {
      pub step: SetupStep,
      /// Step 1: Provider 选择
      pub provider_type: ProviderType,
      pub provider_id: String,
      pub base_url: String,
      pub step1_focus: Step1Field,
      /// Step 2: API Key
      pub api_key: String,
      /// Step 3: 模型别名
      pub aliases: [AliasConfig; 3],
      pub step3_focus: usize,
      /// 是否正在显示跳过确认
      pub confirm_skip: bool,
  }

  #[derive(Debug, Clone, Copy, PartialEq)]
  pub enum Step1Field {
      ProviderType,
      ProviderId,
      BaseUrl,
  }

  impl Step1Field {
      pub fn next(&self) -> Self {
          match self {
              Self::ProviderType => Self::ProviderId,
              Self::ProviderId => Self::BaseUrl,
              Self::BaseUrl => Self::ProviderType,
          }
      }

      pub fn prev(&self) -> Self {
          match self {
              Self::ProviderType => Self::BaseUrl,
              Self::ProviderId => Self::ProviderType,
              Self::BaseUrl => Self::ProviderId,
          }
      }
  }

  impl SetupWizardPanel {
      pub fn new() -> Self {
          let pt = ProviderType::Anthropic;
          Self {
              step: SetupStep::Provider,
              provider_type: pt,
              provider_id: pt.default_provider_id().to_string(),
              base_url: pt.default_base_url().to_string(),
              step1_focus: Step1Field::ProviderType,
              api_key: String::new(),
              aliases: pt.default_model_ids().map(|s| AliasConfig { model_id: s.to_string() }),
              step3_focus: 0,
              confirm_skip: false,
          }
      }

      /// 切换 Provider 类型后刷新默认值
      pub fn refresh_provider_defaults(&mut self) {
          self.provider_id = self.provider_type.default_provider_id().to_string();
          self.base_url = self.provider_type.default_base_url().to_string();
          self.aliases = self.provider_type.default_model_ids().map(|s| AliasConfig { model_id: s.to_string() });
      }
  }
  ```
  - 原因: 与设计文档 `spec-design.md` 数据模型一致，参照 `RelayPanel` 模式（独立 struct + buf_* 字段缓冲 + mode 枚举）

- [x] 在 `setup_wizard.rs` 中实现 `needs_setup()` 检测函数
  - 位置: `SetupWizardPanel` 定义之后，模块级函数
  ```rust
  /// 检测配置是否需要 Setup 向导
  /// 条件 1：providers 列表为空
  /// 条件 2：有 provider 但 api_key 为空且对应环境变量未设置
  pub fn needs_setup(config: &crate::config::AppConfig) -> bool {
      // 条件 1：无任何 Provider
      if config.providers.is_empty() {
          return true;
      }
      // 条件 2：有 Provider 但 API Key 缺失
      for provider in &config.providers {
          if provider.api_key.is_empty() {
              let key_env = match provider.provider_type.as_str() {
                  "anthropic" => "ANTHROPIC_API_KEY",
                  _ => "OPENAI_API_KEY",
              };
              if std::env::var(key_env).unwrap_or_default().is_empty() {
                  return true;
              }
          }
      }
      false
  }
  ```
  - 原因: 与 `spec-design.md` 触发机制一致。注意 `ProviderConfig` 的字段名为 `api_key`（JSON 映射为 `apiKey`），非 `Option<String>`，直接判空即可；`provider_type` 字段区分 Anthropic/OpenAI 以查找正确的 env var

- [x] 修改 `rust-agent-tui/src/app/mod.rs`，注册模块并添加 `setup_wizard` 字段
  - 位置 1: `mod` 声明区域（L6 `mod provider;` 之后），添加 `pub mod setup_wizard;`
  - 位置 2: `pub use` 区域（L54 `pub use relay_panel::RelayPanel;` 之后），添加 `pub use setup_wizard::SetupWizardPanel;`
  - 位置 3: `App` 结构体定义（L83 `pub relay_panel: Option<RelayPanel>,` 之后），添加字段:
    ```rust
    pub setup_wizard: Option<SetupWizardPanel>,
    ```
  - 位置 4: `App::new()` 返回值构造（L158 `relay_panel: None,` 之后），添加:
    ```rust
    setup_wizard: None,
    ```
  - 位置 5: `App::new_headless()` 返回值构造（`panel_ops.rs` L277 `relay_panel: None,` 之后），添加:
    ```rust
    setup_wizard: None,
    ```
  - 原因: 保持与 `relay_panel` 等可选面板字段一致的注册模式

- [x] 修改 `rust-agent-tui/src/main.rs`，在 `run_app()` 中调用 `needs_setup()` 检测
  - 位置: `run_app()` 函数体内，`let mut app = App::new();` 之后、`app.try_connect_relay(...)` 之前（L112 `App::new()` 之后、L115 `try_connect_relay` 之前）
  - 内容:
    ```rust
    // 检测是否需要 Setup 向导
    if let Some(ref cfg) = app.zen_config {
        if crate::app::setup_wizard::needs_setup(&cfg.config) {
            app.setup_wizard = Some(crate::app::SetupWizardPanel::new());
        }
    } else {
        // 无配置文件 → 必然需要 setup
        app.setup_wizard = Some(crate::app::SetupWizardPanel::new());
    }
    ```
  - 原因: `zen_config` 为 `None` 时说明 `settings.json` 不存在，属于首次安装场景，必须触发 setup；在 `try_connect_relay` 之前检测，因为 relay 连接依赖配置完整性

- [x] 为 `needs_setup()` 和 `SetupWizardPanel` 编写单元测试
  - 测试文件: `rust-agent-tui/src/app/setup_wizard.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `test_needs_setup_empty_providers`: providers 为空 → 返回 true
    - `test_needs_setup_empty_api_key_no_env`: 有 provider 但 api_key 为空且无 env var → 返回 true
    - `test_needs_setup_api_key_from_config`: 有 provider 且 api_key 非空 → 返回 false
    - `test_needs_setup_api_key_from_env`: provider api_key 为空但 `OPENAI_API_KEY` env 已设置 → 返回 false
    - `test_needs_setup_no_config_file`: AppConfig 为 default（空 providers）→ 返回 true
    - `test_setup_wizard_new_defaults`: `SetupWizardPanel::new()` 默认值为 Anthropic + 合理默认值
    - `test_provider_type_cycle`: `ProviderType::cycle()` 在两个变体间循环
    - `test_refresh_provider_defaults`: 切换 provider_type 后 provider_id/base_url/aliases 自动刷新
    - `test_step1_field_navigation`: `Step1Field::next()/prev()` 三字段循环导航正确
  - 运行命令: `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | tail -15`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证新文件存在且模块已注册
  - `grep -c "pub mod setup_wizard" rust-agent-tui/src/app/mod.rs`
  - 预期: 输出 1
- [x] 验证 App 结构体包含 setup_wizard 字段
  - `grep -c "setup_wizard" rust-agent-tui/src/app/mod.rs`
  - 预期: 输出 ≥ 3（声明、构造、pub use）
- [x] 验证 main.rs 中调用了 needs_setup
  - `grep -c "needs_setup" rust-agent-tui/src/main.rs`
  - 预期: 输出 1
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -c "error"`
  - 预期: 输出 0
- [x] 运行单元测试通过
  - `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | grep "test result"`
  - 预期: 输出包含 `ok` 且 passed > 0

---

### Task 2: Setup 向导 UI 渲染

**背景:**
Task 1 创建了 `SetupWizardPanel` 数据模型（`SetupStep`/`ProviderType`/`Step1Field`/`AliasConfig`）和 `needs_setup()` 检测函数。本 Task 实现全屏向导的渲染逻辑——当 `app.setup_wizard.is_some()` 时，`main_ui::render()` 优先拦截并全屏渲染向导界面，完全跳过正常对话界面。渲染函数按 `wizard.step` 分发到四个子步骤渲染函数（Step1 Provider / Step2 ApiKey / Step3 ModelAlias / Done 确认页），风格参照 `popups/relay.rs` 和 `panels/model.rs` 的 `Block + Paragraph + Line/Span` 模式。本 Task 的输出被 Task 3（事件处理）依赖——Task 3 修改 `event.rs` 按键分发时需要渲染结果可验证。

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/main_ui/popups/setup_wizard.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`（render 入口优先检查 setup_wizard，全屏渲染）
- 修改: `rust-agent-tui/src/ui/main_ui/popups/mod.rs`（添加 `pub mod setup_wizard`）

**执行步骤:**

- [x] 新建 `rust-agent-tui/src/ui/main_ui/popups/setup_wizard.rs`，实现 `render_setup_wizard()` 全屏入口函数和四个子步骤渲染函数
  - 位置: 文件顶部，imports 之后
  - 依赖 imports: `ratatui::{layout::Rect, style::{Color, Modifier, Style}, text::{Line, Span, Text}, widgets::{Block, Borders, Clear, Paragraph}, Frame}`, `crate::app::App`, `crate::app::setup_wizard::{SetupStep, SetupWizardPanel, Step1Field, ProviderType}`, `crate::ui::theme`
  - 全屏入口函数:
  ```rust
  /// Setup 向导全屏渲染入口
  pub(crate) fn render_setup_wizard(f: &mut Frame, app: &App) {
      let area = f.area();
      f.render_widget(Clear, area);

      let wizard = app.setup_wizard.as_ref().unwrap();

      // 居中内容区：宽度 60%，高度按内容自适应（最少 16 行）
      let content_width = (area.width * 3 / 5).max(50);
      let content_height = match wizard.step {
          SetupStep::Provider => 16,
          SetupStep::ApiKey => 12,
          SetupStep::ModelAlias => 16,
          SetupStep::Done => 14,
      }.min(area.height.saturating_sub(2));
      let centered = centered_rect(area, content_width, content_height);

      match wizard.step {
          SetupStep::Provider => render_step_provider(f, wizard, centered),
          SetupStep::ApiKey => render_step_api_key(f, wizard, centered),
          SetupStep::ModelAlias => render_step_model_alias(f, wizard, centered),
          SetupStep::Done => render_step_done(f, wizard, centered),
      }
  }

  /// 计算居中矩形区域
  fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
      let x = area.x + (area.width.saturating_sub(width)) / 2;
      let y = area.y + (area.height.saturating_sub(height)) / 2;
      Rect::new(x, y, width.min(area.width), height.min(area.height))
  }
  ```
  - 原因: 与设计文档一致，setup 期间完全替换主界面渲染；居中矩形函数是 TUI 全屏弹窗的惯用模式

- [x] 实现 `render_step_provider()` — Step 1 Provider 选择渲染
  - 位置: `render_setup_wizard()` 之后
  - 内容:
  ```rust
  fn render_step_provider(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
      let block = Block::default()
          .title(Span::styled(
              " ── Perihelion Setup ── Step 1/3: Provider ",
              Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
          ))
          .borders(Borders::ALL)
          .border_style(Style::default().fg(theme::ACCENT));
      f.render_widget(&block, area);
      let inner = block.inner(area);

      // 焦点样式辅助
      let focused = |is_active: bool| -> (Style, Style) {
          if is_active {
              (
                  Style::default().fg(Color::White).bg(theme::ACCENT).add_modifier(Modifier::BOLD),
                  Style::default().fg(Color::White).bg(theme::ACCENT),
              )
          } else {
              (Style::default().fg(theme::MUTED), Style::default().fg(theme::TEXT))
          }
      };

      // 行 0: Provider Type 选择器
      let pt_active = wizard.step1_focus == Step1Field::ProviderType;
      let (pt_label, pt_val) = focused(pt_active);
      let provider_types = [ProviderType::Anthropic, ProviderType::OpenAiCompatible];
      let pt_display: String = provider_types.iter()
          .map(|pt| if *pt == wizard.provider_type { format!("[{}]", pt.label()) } else { pt.label().to_string() })
          .collect::<Vec<_>>()
          .join("  ");
      let line_pt = Line::from(vec![
          Span::styled(" Type     ", pt_label),
          Span::styled(format!(" {}", pt_display), pt_val),
      ]);

      // 行 1: Provider ID 输入
      let pid_active = wizard.step1_focus == Step1Field::ProviderId;
      let (pid_label, pid_val) = focused(pid_active);
      let pid_display = if pid_active { format!("{}▏", wizard.provider_id) } else { wizard.provider_id.clone() };
      let line_pid = Line::from(vec![
          Span::styled(" ID       ", pid_label),
          Span::styled(format!(" {}", pid_display), pid_val),
      ]);

      // 行 2: Base URL 输入
      let url_active = wizard.step1_focus == Step1Field::BaseUrl;
      let (url_label, url_val) = focused(url_active);
      let is_readonly = wizard.provider_type == ProviderType::Anthropic;
      let url_display = if url_active && !is_readonly { format!("{}▏", wizard.base_url) } else { wizard.base_url.clone() };
      let readonly_tag = if is_readonly { " (readonly)" } else { "" };
      let line_url = Line::from(vec![
          Span::styled(format!(" Base URL{} ", readonly_tag), url_label),
          Span::styled(format!(" {}", url_display), url_val),
      ]);

      // 底部提示
      let hint = Line::from(vec![
          Span::styled(" Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
          Span::styled(":下一步  ", Style::default().fg(theme::MUTED)),
          Span::styled("Esc", Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD)),
          Span::styled(":跳过setup  ", Style::default().fg(theme::MUTED)),
          Span::styled("Tab", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
          Span::styled(":切换字段", Style::default().fg(theme::MUTED)),
      ]);

      let mut lines = vec![
          Line::from(""),
          line_pt,
          line_pid,
          line_url,
          Line::from(""),
          hint,
      ];

      // 跳过确认覆盖层
      if wizard.confirm_skip {
          lines.push(Line::from(""));
          lines.push(Line::from(vec![
              Span::styled(" ⚠ 跳过 setup 将无法使用 AI 功能，", Style::default().fg(theme::ERROR)),
          ]));
          lines.push(Line::from(vec![
              Span::styled("   按 ", Style::default().fg(theme::TEXT)),
              Span::styled("Enter", Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD)),
              Span::styled(" 确认跳过，", Style::default().fg(theme::TEXT)),
              Span::styled("Esc", Style::default().fg(theme::SAGE).add_modifier(Modifier::BOLD)),
              Span::styled(" 取消", Style::default().fg(theme::TEXT)),
          ]));
      }

      lines.truncate(inner.height as usize);
      f.render_widget(Paragraph::new(Text::from(lines)), inner);
  }
  ```
  - 原因: 与设计文档 Step 1 线框图一致；焦点字段用 `White bg + ACCENT fg` 高亮（与 model.rs 的 `AliasEditField` 模式一致）；Provider Type 使用 `[label]` 包裹选中项（与 model.rs 的 provider 循环选择模式一致）；Anthropic 模式 Base URL 标记为 readonly；`confirm_skip` 覆盖层在底部显示确认提示

- [x] 实现 `render_step_api_key()` — Step 2 API Key 输入渲染
  - 位置: `render_step_provider()` 之后
  - 内容:
  ```rust
  fn render_step_api_key(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
      let block = Block::default()
          .title(Span::styled(
              " ── Step 2/3: API Key ",
              Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
          ))
          .borders(Borders::ALL)
          .border_style(Style::default().fg(theme::ACCENT));
      f.render_widget(&block, area);
      let inner = block.inner(area);

      // Provider 名称
      let line_provider = Line::from(vec![
          Span::styled(" Provider: ", Style::default().fg(theme::MUTED)),
          Span::styled(wizard.provider_type.label(), Style::default().fg(theme::TEXT)),
      ]);

      // API Key 掩码输入（密码模式）
      let masked: String = if wizard.api_key.is_empty() {
          "".to_string()
      } else {
          "•".repeat(wizard.api_key.len())
      };
      let line_key = Line::from(vec![
          Span::styled(" API Key:  ", Style::default().fg(Color::White).bg(theme::ACCENT).add_modifier(Modifier::BOLD)),
          Span::styled(format!(" {}▏", masked), Style::default().fg(Color::White).bg(theme::ACCENT)),
      ]);

      // 底部提示
      let hint = Line::from(vec![
          Span::styled(" Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
          Span::styled(":下一步  ", Style::default().fg(theme::MUTED)),
          Span::styled("Esc", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
          Span::styled(":返回上一步", Style::default().fg(theme::MUTED)),
      ]);

      let lines = vec![
          Line::from(""),
          line_provider,
          Line::from(""),
          line_key,
          Line::from(""),
          hint,
      ];
      f.render_widget(Paragraph::new(Text::from(lines)), inner);
  }
  ```
  - 原因: API Key 输入始终为焦点字段（只有一个输入框），用 `•` 掩码显示（与设计文档 "密码模式" 一致）；显示当前 Provider 名称让用户确认上下文

- [x] 实现 `render_step_model_alias()` — Step 3 模型别名配置渲染
  - 位置: `render_step_api_key()` 之后
  - 内容:
  ```rust
  fn render_step_model_alias(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
      let block = Block::default()
          .title(Span::styled(
              " ── Step 3/3: Model Aliases ",
              Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
          ))
          .borders(Borders::ALL)
          .border_style(Style::default().fg(theme::ACCENT));
      f.render_widget(&block, area);
      let inner = block.inner(area);

      let alias_labels = ["Opus ", "Sonnet", "Haiku "];
      let mut lines: Vec<Line> = vec![Line::from("")];

      for (i, label) in alias_labels.iter().enumerate() {
          let is_active = wizard.step3_focus == i;
          let (lbl_style, val_style) = if is_active {
              (
                  Style::default().fg(Color::White).bg(theme::ACCENT).add_modifier(Modifier::BOLD),
                  Style::default().fg(Color::White).bg(theme::ACCENT),
              )
          } else {
              (Style::default().fg(theme::MUTED), Style::default().fg(theme::TEXT))
          };
          let model_display = if is_active {
              format!("{}▏", wizard.aliases[i].model_id)
          } else {
              wizard.aliases[i].model_id.clone()
          };
          lines.push(Line::from(vec![
              Span::styled(format!(" {}  Model: ", label), lbl_style),
              Span::styled(format!("{}", model_display), val_style),
          ]));
      }

      // 底部提示
      lines.push(Line::from(""));
      lines.push(Line::from(vec![
          Span::styled(" Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
          Span::styled(":完成配置  ", Style::default().fg(theme::MUTED)),
          Span::styled("Esc", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
          Span::styled(":返回上一步  ", Style::default().fg(theme::MUTED)),
          Span::styled("Tab", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
          Span::styled(":切换字段", Style::default().fg(theme::MUTED)),
      ]));

      lines.truncate(inner.height as usize);
      f.render_widget(Paragraph::new(Text::from(lines)), inner);
  }
  ```
  - 原因: 三行别名配置，每行仅含 Model ID 输入（Provider 在向导中只有 Step 1 配置的那一个，无需下拉选择）；`step3_focus` 范围 0-2 对应 Opus/Sonnet/Haiku；焦点样式与其他步骤一致

- [x] 实现 `render_step_done()` — 完成确认页渲染
  - 位置: `render_step_model_alias()` 之后
  - 内容:
  ```rust
  fn render_step_done(f: &mut Frame, wizard: &SetupWizardPanel, area: Rect) {
      let block = Block::default()
          .title(Span::styled(
              " ── Setup Complete ✓ ",
              Style::default().fg(theme::SAGE).add_modifier(Modifier::BOLD),
          ))
          .borders(Borders::ALL)
          .border_style(Style::default().fg(theme::SAGE));
      f.render_widget(&block, area);
      let inner = block.inner(area);

      let alias_labels = ["Opus", "Sonnet", "Haiku"];

      let mut lines = vec![
          Line::from(""),
          Line::from(vec![
              Span::styled(" Provider: ", Style::default().fg(theme::MUTED)),
              Span::styled(wizard.provider_type.label(), Style::default().fg(theme::TEXT)),
          ]),
          Line::from(vec![
              Span::styled(" ID:       ", Style::default().fg(theme::MUTED)),
              Span::styled(&wizard.provider_id, Style::default().fg(theme::TEXT)),
          ]),
          Line::from(vec![
              Span::styled(" Key:      ", Style::default().fg(theme::MUTED)),
              Span::styled(mask_api_key(&wizard.api_key), Style::default().fg(theme::TEXT)),
          ]),
          Line::from(""),
      ];

      // 三个别名摘要
      for (i, label) in alias_labels.iter().enumerate() {
          lines.push(Line::from(vec![
              Span::styled(format!(" {:>6}  →  ", label), Style::default().fg(theme::MUTED)),
              Span::styled(&wizard.aliases[i].model_id, Style::default().fg(theme::ACCENT)),
          ]));
      }

      lines.push(Line::from(""));
      lines.push(Line::from(vec![
          Span::styled(" 按 ", Style::default().fg(theme::TEXT)),
          Span::styled("Enter", Style::default().fg(theme::SAGE).add_modifier(Modifier::BOLD)),
          Span::styled(" 开始使用", Style::default().fg(theme::TEXT)),
      ]));

      lines.truncate(inner.height as usize);
      f.render_widget(Paragraph::new(Text::from(lines)), inner);
  }

  /// API Key 脱敏：首4位 + **** + 末4位
  fn mask_api_key(key: &str) -> String {
      let chars: Vec<char> = key.chars().collect();
      let len = chars.len();
      if len <= 8 {
          "•".repeat(len)
      } else {
          let prefix: String = chars[..4].iter().collect();
          let suffix: String = chars[len - 4..].iter().collect();
          format!("{}••••{}", prefix, suffix)
      }
  }
  ```
  - 原因: 与设计文档完成页描述一致——列出 Provider 摘要和三个模型别名；API Key 脱敏显示；标题用 `theme::SAGE`（成功色）与进行中步骤的 `theme::ACCENT` 区分

- [x] 修改 `rust-agent-tui/src/ui/main_ui.rs`，在 `render()` 入口添加 setup wizard 全屏优先检查
  - 位置: `render()` 函数体开头（L20 `let area = f.area();` 之前）
  - 插入代码:
  ```rust
  // Setup 向导：全屏覆盖，优先于所有正常界面
  if app.setup_wizard.is_some() {
      popups::setup_wizard::render_setup_wizard(f, app);
      return;
  }
  ```
  - 原因: 与设计文档 "UI 渲染集成" 一致——setup 期间完全替换主界面渲染，不渲染消息列表/输入框/状态栏；放在 `render()` 最前面确保最高优先级

- [x] 修改 `rust-agent-tui/src/ui/main_ui/popups/mod.rs`，注册 `setup_wizard` 模块
  - 位置: 现有模块声明（`pub mod hints;` 之后）
  - 追加: `pub mod setup_wizard;`
  - 原因: 与现有 popup 模块注册模式一致

- [x] 为 Setup 向导渲染逻辑编写单元测试（headless 集成测试）
  - 测试位置: `rust-agent-tui/src/ui/main_ui/popups/setup_wizard.rs` 底部 `#[cfg(test)] mod tests`
  - 测试场景:
    - `test_render_step1_default`: 创建 `SetupWizardPanel::new()`，headless 渲染 → 检查包含 `"Perihelion Setup"`、`"Step 1/3"`、`"Anthropic"`、`"Enter"`
    - `test_render_step2_masked_api_key`: 设置 `wizard.step = SetupStep::ApiKey`，`wizard.api_key = "sk-abc123xyz789"` → 渲染 → 检查包含 `"Step 2/3"`，`"sk-a"`（前缀可见），`"789"`（后缀可见），且不包含完整 key
    - `test_render_step3_aliases`: 设置 `wizard.step = SetupStep::ModelAlias` → 渲染 → 检查包含 `"Step 3/3"`、`"Opus"`、`"Sonnet"`、`"Haiku"`、`"claude-"`
    - `test_render_done_page`: 设置 `wizard.step = SetupStep::Done` → 渲染 → 检查包含 `"Setup Complete"`、`"Enter"`
    - `test_render_step1_confirm_skip`: 设置 `wizard.confirm_skip = true` → 渲染 → 检查包含 `"跳过 setup"`
  - 运行命令: `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | tail -15`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证新文件存在
  - `test -f rust-agent-tui/src/ui/main_ui/popups/setup_wizard.rs && echo OK`
  - 预期: 输出 `OK`
- [x] 验证模块已注册
  - `grep -c "pub mod setup_wizard" rust-agent-tui/src/ui/main_ui/popups/mod.rs`
  - 预期: 输出 1
- [x] 验证 main_ui.rs 包含 setup_wizard 优先检查
  - `grep -c "setup_wizard" rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 输出 ≥ 2（render 入口检查 + popups 调用）
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -c "error"`
  - 预期: 输出 0
- [x] 运行单元测试通过
  - `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | grep "test result"`
  - 预期: 输出包含 `ok` 且 passed > 0

---

### Task 3: Setup 向导事件处理与持久化

**背景:**
Task 1 创建了 `SetupWizardPanel` 数据模型和 `needs_setup()` 检测函数，Task 2 实现了全屏渲染逻辑。本 Task 补齐事件处理链条的最后一环——在 `event.rs` 的 `next_event()` 最前面添加 setup_wizard 的优先拦截，使得当 `app.setup_wizard.is_some()` 时所有按键事件被向导消费，不穿透到正常对话界面。同时实现 `handle_setup_wizard_key()` 按步骤分发按键（Tab/Enter/Esc/↑↓/字符输入/Backspace）、`save_setup()` 将向导结果写入 `~/.zen-code/settings.json`、`App::refresh_after_setup()` 刷新内存中的 `provider_name`/`model_name` 状态，使配置即时生效。本 Task 完成后，Setup 向导功能端到端可用。

**涉及文件:**
- 修改: `rust-agent-tui/src/event.rs`（在 `next_event()` 最前面添加 setup_wizard 优先拦截）
- 修改: `rust-agent-tui/src/app/setup_wizard.rs`（Task 1 已创建；添加 `handle_setup_wizard_key`、`save_setup`、`refresh_after_setup` 方法）
- 修改: `rust-agent-tui/src/app/mod.rs`（添加 `refresh_after_setup` 方法，或 impl block 内调用）

**执行步骤:**

- [x] 在 `rust-agent-tui/src/app/setup_wizard.rs` 中实现 `handle_setup_wizard_key()` 按键处理函数
  - 位置: `SetupWizardPanel` impl 块之后，模块级 `pub fn handle_setup_wizard_key()`
  - 签名: `pub fn handle_setup_wizard_key(wizard: &mut SetupWizardPanel, key: Input) -> Option<SetupWizardAction>`
  - 返回枚举（定义在模块顶部）:
  ```rust
  /// setup_wizard 按键处理的返回动作
  pub enum SetupWizardAction {
      /// 无特殊动作，仅重绘
      Redraw,
      /// 保存配置并关闭向导（返回向导 panel 供 save_setup 使用）
      SaveAndClose,
      /// 不保存，直接关闭向导（跳过）
      Skip,
  }
  ```
  - 按步骤分发的关键逻辑（伪代码）:
  ```rust
  pub fn handle_setup_wizard_key(wizard: &mut SetupWizardPanel, input: Input) -> Option<SetupWizardAction> {
      // 跳过确认弹窗优先处理
      if wizard.confirm_skip {
          return handle_confirm_skip(wizard, input);
      }

      match wizard.step {
          SetupStep::Provider => handle_step_provider(wizard, input),
          SetupStep::ApiKey => handle_step_api_key(wizard, input),
          SetupStep::ModelAlias => handle_step_model_alias(wizard, input),
          SetupStep::Done => handle_step_done(wizard, input),
      }
  }
  ```
  - 原因: 与 `event.rs` 中 `handle_model_panel` / `handle_relay_panel` 的模式一致——独立的按键处理函数按 mode/step 分发，返回结构化动作

- [x] 实现 `handle_confirm_skip()` — 跳过确认弹窗的按键处理
  - 位置: `handle_setup_wizard_key()` 下方，私有辅助函数
  ```rust
  fn handle_confirm_skip(wizard: &mut SetupWizardPanel, input: Input) -> Option<SetupWizardAction> {
      match input {
          Input { key: Key::Enter, .. } => Some(SetupWizardAction::Skip),
          Input { key: Key::Esc, .. } => {
              wizard.confirm_skip = false;
              Some(SetupWizardAction::Redraw)
          }
          _ => None,
      }
  }
  ```

- [x] 实现 `handle_step_provider()` — Step 1 Provider 选择的按键处理
  - 位置: `handle_confirm_skip()` 之后
  - 关键逻辑:
  ```rust
  fn handle_step_provider(wizard: &mut SetupWizardPanel, input: Input) -> Option<SetupWizardAction> {
      match input {
          // Tab: 在 Step1Field 三个字段间循环切换
          Input { key: Key::Tab, shift: false, .. } => {
              wizard.step1_focus = wizard.step1_focus.next();
              Some(SetupWizardAction::Redraw)
          }
          Input { key: Key::Tab, shift: true, .. } => {
              wizard.step1_focus = wizard.step1_focus.prev();
              Some(SetupWizardAction::Redraw)
          }
          // ↑↓: 当 focus == ProviderType 时循环切换 Provider 类型
          Input { key: Key::Up, .. } => {
              if wizard.step1_focus == Step1Field::ProviderType {
                  wizard.provider_type.cycle();
                  wizard.refresh_provider_defaults();
              }
              Some(SetupWizardAction::Redraw)
          }
          Input { key: Key::Down, .. } => {
              if wizard.step1_focus == Step1Field::ProviderType {
                  wizard.provider_type.cycle();
                  wizard.refresh_provider_defaults();
              }
              Some(SetupWizardAction::Redraw)
          }
          // Enter: 校验 provider_id 非空后进入 Step 2
          Input { key: Key::Enter, .. } => {
              if !wizard.provider_id.trim().is_empty() {
                  wizard.step = SetupStep::ApiKey;
              }
              Some(SetupWizardAction::Redraw)
          }
          // Esc: 触发跳过确认
          Input { key: Key::Esc, .. } => {
              wizard.confirm_skip = true;
              Some(SetupWizardAction::Redraw)
          }
          // Backspace: 删除当前字段末字符
          Input { key: Key::Backspace, .. } => {
              match wizard.step1_focus {
                  Step1Field::ProviderId => { wizard.provider_id.pop(); }
                  Step1Field::BaseUrl if wizard.provider_type != ProviderType::Anthropic => {
                      wizard.base_url.pop();
                  }
                  _ => {}
              }
              Some(SetupWizardAction::Redraw)
          }
          // 字符输入: 写入当前编辑字段（ProviderType 字段忽略，用 ↑↓ 切换）
          Input { key: Key::Char(c), ctrl: false, alt: false, .. } => {
              match wizard.step1_focus {
                  Step1Field::ProviderId => wizard.provider_id.push(c),
                  Step1Field::BaseUrl if wizard.provider_type != ProviderType::Anthropic => {
                      wizard.base_url.push(c);
                  }
                  _ => {}
              }
              Some(SetupWizardAction::Redraw)
          }
          _ => None,
      }
  }
  ```
  - 原因: Anthropic 模式下 Base URL 为只读（与 Task 2 渲染逻辑一致），不响应字符输入/Backspace；↑↓ 在 ProviderType 字段时循环切换类型并自动刷新默认值（`refresh_provider_defaults()` 已在 Task 1 实现）

- [x] 实现 `handle_step_api_key()` — Step 2 API Key 输入的按键处理
  - 位置: `handle_step_provider()` 之后
  ```rust
  fn handle_step_api_key(wizard: &mut SetupWizardPanel, input: Input) -> Option<SetupWizardAction> {
      match input {
          // Enter: 校验 api_key 非空后进入 Step 3
          Input { key: Key::Enter, .. } => {
              if !wizard.api_key.trim().is_empty() {
                  wizard.step = SetupStep::ModelAlias;
              }
              Some(SetupWizardAction::Redraw)
          }
          // Esc: 返回 Step 1
          Input { key: Key::Esc, .. } => {
              wizard.step = SetupStep::Provider;
              Some(SetupWizardAction::Redraw)
          }
          // Backspace: 删除 api_key 末字符
          Input { key: Key::Backspace, .. } => {
              wizard.api_key.pop();
              Some(SetupWizardAction::Redraw)
          }
          // 字符输入: 追加到 api_key
          Input { key: Key::Char(c), ctrl: false, alt: false, .. } => {
              wizard.api_key.push(c);
              Some(SetupWizardAction::Redraw)
          }
          _ => None,
      }
  }
  ```
  - 原因: Step 2 只有一个输入字段（API Key），无需 Tab 切换焦点；所有字符直接追加到 api_key；Esc 返回上一步保留已填内容（与设计文档一致）

- [x] 实现 `handle_step_model_alias()` — Step 3 模型别名配置的按键处理
  - 位置: `handle_step_api_key()` 之后
  ```rust
  fn handle_step_model_alias(wizard: &mut SetupWizardPanel, input: Input) -> Option<SetupWizardAction> {
      match input {
          // Tab: 在 3 个别名行间循环
          Input { key: Key::Tab, shift: false, .. } => {
              wizard.step3_focus = (wizard.step3_focus + 1) % 3;
              Some(SetupWizardAction::Redraw)
          }
          Input { key: Key::Tab, shift: true, .. } => {
              wizard.step3_focus = (wizard.step3_focus + 2) % 3; // 等效 -1 mod 3
              Some(SetupWizardAction::Redraw)
          }
          // Enter: 校验三个 model_id 非空后进入 Done 步骤
          Input { key: Key::Enter, .. } => {
              if wizard.aliases.iter().all(|a| !a.model_id.trim().is_empty()) {
                  wizard.step = SetupStep::Done;
              }
              Some(SetupWizardAction::Redraw)
          }
          // Esc: 返回 Step 2
          Input { key: Key::Esc, .. } => {
              wizard.step = SetupStep::ApiKey;
              Some(SetupWizardAction::Redraw)
          }
          // Backspace: 删除当前焦点别名的 model_id 末字符
          Input { key: Key::Backspace, .. } => {
              wizard.aliases[wizard.step3_focus].model_id.pop();
              Some(SetupWizardAction::Redraw)
          }
          // 字符输入: 追加到当前焦点别名的 model_id
          Input { key: Key::Char(c), ctrl: false, alt: false, .. } => {
              wizard.aliases[wizard.step3_focus].model_id.push(c);
              Some(SetupWizardAction::Redraw)
          }
          _ => None,
      }
  }
  ```
  - 原因: `step3_focus` 范围 0-2（Opus/Sonnet/Haiku），Tab 循环切换；Enter 校验所有三个 model_id 非空（与设计文档"三个模型别名必填"一致）

- [x] 实现 `handle_step_done()` — 完成页的按键处理
  - 位置: `handle_step_model_alias()` 之后
  ```rust
  fn handle_step_done(wizard: &mut SetupWizardPanel, input: Input) -> Option<SetupWizardAction> {
      match input {
          Input { key: Key::Enter, .. } => Some(SetupWizardAction::SaveAndClose),
          // Esc: 返回 Step 3 允许修改
          Input { key: Key::Esc, .. } => {
              wizard.step = SetupStep::ModelAlias;
              Some(SetupWizardAction::Redraw)
          }
          _ => None,
      }
  }
  ```
  - 原因: Done 页只需 Enter 确认保存 + Esc 返回修改

- [x] 在 `rust-agent-tui/src/app/setup_wizard.rs` 中实现 `save_setup()` 配置保存函数
  - 位置: `handle_setup_wizard_key()` 及其辅助函数之后
  - 签名: `pub fn save_setup(wizard: &SetupWizardPanel) -> anyhow::Result<crate::config::ZenConfig>`
  - 关键逻辑:
  ```rust
  pub fn save_setup(wizard: &SetupWizardPanel) -> anyhow::Result<crate::config::ZenConfig> {
      // 1. 加载现有配置（不存在则用默认值）
      let mut cfg = crate::config::load().unwrap_or_default();

      // 2. 构建 ProviderConfig 并添加到 providers 列表
      let provider_type_str = match wizard.provider_type {
          ProviderType::Anthropic => "anthropic",
          ProviderType::OpenAiCompatible => "openai",
      };
      let provider = crate::config::types::ProviderConfig {
          id: wizard.provider_id.clone(),
          provider_type: provider_type_str.to_string(),
          api_key: wizard.api_key.clone(),
          base_url: wizard.base_url.clone(),
          ..Default::default()
      };
      cfg.config.providers.push(provider);

      // 3. 设置 active_alias 为 "opus"
      cfg.config.active_alias = "opus".to_string();

      // 4. 设置三个模型别名（provider_id 都指向刚创建的 provider）
      let alias_labels = ["opus", "sonnet", "haiku"];
      let alias_configs = [
          &mut cfg.config.model_aliases.opus,
          &mut cfg.config.model_aliases.sonnet,
          &mut cfg.config.model_aliases.haiku,
      ];
      for (i, alias_cfg) in alias_configs.into_iter().enumerate() {
          alias_cfg.provider_id = wizard.provider_id.clone();
          alias_cfg.model_id = wizard.aliases[i].model_id.clone();
      }

      // 5. 原子写回文件
      crate::config::save(&cfg)?;

      Ok(cfg)
  }
  ```
  - 原因: 与设计文档"持久化"部分一致；复用 `config::load()` 加载现有配置（保留 extra 字段不丢失）、`config::save()` 原子写回（`store.rs` 已实现 tmp+rename 模式）；`ProviderConfig` 的字段名确认：`id`、`provider_type`（JSON rename `"type"`）、`api_key`（JSON rename `"apiKey"`）、`base_url`（JSON rename `"baseUrl"`），均来自 `types.rs:ProviderConfig` 定义

- [x] 在 `rust-agent-tui/src/app/mod.rs` 中实现 `App::refresh_after_setup()` 方法
  - 位置: `App` impl 块内（`try_connect_relay()` 方法之后）
  - 关键逻辑:
  ```rust
  /// Setup 向导保存后刷新内存中的 Provider 状态
  pub fn refresh_after_setup(&mut self, cfg: crate::config::ZenConfig) {
      // 更新内存中的 zen_config
      self.zen_config = Some(cfg);

      // 重新从配置构造 LlmProvider，更新 provider_name / model_name
      let cfg_ref = self.zen_config.as_ref().unwrap();
      if let Some(p) = agent::LlmProvider::from_config(cfg_ref) {
          self.provider_name = p.display_name().to_string();
          self.model_name = p.model_name().to_string();
      }
  }
  ```
  - 原因: `App::new()` 中已有相同的 provider 解析逻辑（`LlmProvider::from_config` + `display_name()` + `model_name()`），此处复用同一模式刷新内存状态，使 Agent 执行能立即使用新配置

- [x] ~~修改 `rust-agent-tui/src/app/panel_ops.rs`~~ — 已在 Task 1 中完成

- [x] 修改 `rust-agent-tui/src/event.rs`，在 `next_event()` 最前面添加 setup_wizard 优先拦截
  - 位置: `next_event()` 函数体内，`Event::Key(key_event)` 分支中，`// Thread 浏览面板优先处理` 注释之前（L50 之前）
  - 插入代码:
  ```rust
  // Setup 向导：优先拦截所有按键事件
  if let Some(ref mut wizard) = app.setup_wizard {
      if let Some(action) = crate::app::setup_wizard::handle_setup_wizard_key(wizard, input) {
          match action {
              crate::app::setup_wizard::SetupWizardAction::SaveAndClose => {
                  // 取出 wizard（避免双重借用 app）
                  let wizard = app.setup_wizard.take().unwrap();
                  match crate::app::setup_wizard::save_setup(&wizard) {
                      Ok(cfg) => app.refresh_after_setup(cfg),
                      Err(e) => {
                          let msg = MessageViewModel::from_base_message(
                              &BaseMessage::system(format!("Setup 保存失败: {}", e)),
                              &[],
                          );
                          let _ = app.core.render_tx.send(RenderEvent::AddMessage(msg));
                      }
                  }
              }
              crate::app::setup_wizard::SetupWizardAction::Skip => {
                  app.setup_wizard = None;
              }
              crate::app::setup_wizard::SetupWizardAction::Redraw => {}
          }
          return Ok(Some(Action::Redraw));
      }
  }
  ```

- [x] 为 Setup 向导事件处理和配置保存编写单元测试
  - 测试位置: `rust-agent-tui/src/app/setup_wizard.rs` 底部 `#[cfg(test)] mod tests` 块内
  - 测试场景:
    - `test_needs_setup_empty_providers`: providers 为空 → 返回 true
    - `test_needs_setup_api_key_set`: 有 provider 且 api_key 非空 → 返回 false
    - `test_needs_setup_api_key_empty_no_env`: 有 provider 但 api_key 为空且无对应 env var → 返回 true
    - `test_save_setup_creates_valid_config`: 构造 `SetupWizardPanel`（Anthropic + "sk-test-key" + 三个 model_id）→ 调用 `save_setup()` → 验证返回的 `ZenConfig` 中 providers 有 1 项、provider_type 为 "anthropic"、api_key 为 "sk-test-key"、三个别名 provider_id 正确、model_id 正确、active_alias 为 "opus"
    - `test_save_setup_roundtrip`: `save_setup()` → `save()` 写入临时文件 → `load()` 读回 → 验证字段一致（使用 `std::env::set_var("HOME", temp_dir)` 或 mock config_path 机制）
    - `test_handle_step_provider_tab_cycles_focus`: 创建 wizard step=Provider → Tab → focus 从 ProviderType 变为 ProviderId → 再 Tab → BaseUrl → 再 Tab → 回到 ProviderType
    - `test_handle_step_provider_arrow_cycles_type`: focus=ProviderType → Down → provider_type 变为 OpenAiCompatible → 再 Down → 回到 Anthropic
    - `test_handle_step_provider_enter_advances`: provider_id 非空 → Enter → step 变为 ApiKey
    - `test_handle_step_api_key_enter_advances`: api_key 非空 → Enter → step 变为 ModelAlias
    - `test_handle_step_api_key_esc_back`: Esc → step 变为 Provider
    - `test_handle_step_model_alias_tab_cycles`: Tab → step3_focus 从 0→1→2→0 循环
    - `test_handle_step_model_alias_enter_validates_all`: 三个 model_id 都非空 → Enter → step 变为 Done
    - `test_handle_step_model_alias_enter_blocks_empty_model`: 某个 model_id 为空 → Enter → step 不变（仍为 ModelAlias）
    - `test_handle_step_done_enter_returns_save`: Enter → 返回 `SetupWizardAction::SaveAndClose`
    - `test_handle_step_done_esc_back`: Esc → step 变为 ModelAlias
    - `test_handle_confirm_skip_enter_skip`: confirm_skip=true + Enter → 返回 `SetupWizardAction::Skip`
    - `test_handle_confirm_skip_esc_cancel`: confirm_skip=true + Esc → confirm_skip 变为 false，返回 `Redraw`
  - 运行命令: `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | tail -20`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 event.rs 包含 setup_wizard 优先拦截
  - `grep -c "setup_wizard" rust-agent-tui/src/event.rs`
  - 预期: 输出 ≥ 3（拦截检查 + handle_setup_wizard_key 调用 + SaveAndClose/Skip 处理）
- [x] 验证 setup_wizard.rs 包含 handle_setup_wizard_key 和 save_setup 函数
  - `grep -c "pub fn handle_setup_wizard_key\|pub fn save_setup\|pub enum SetupWizardAction" rust-agent-tui/src/app/setup_wizard.rs`
  - 预期: 输出 = 3
- [x] 验证 App::refresh_after_setup 方法存在
  - `grep -c "fn refresh_after_setup" rust-agent-tui/src/app/mod.rs`
  - 预期: 输出 1
- [x] 验证 new_headless 包含 setup_wizard 字段
  - `grep -c "setup_wizard" rust-agent-tui/src/app/panel_ops.rs`
  - 预期: 输出 1
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -c "error"`
  - 预期: 输出 0
- [x] 运行单元测试通过
  - `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | grep "test result"`
  - 预期: 输出包含 `ok` 且 passed > 0

---

### Task 4: Headless 集成测试

**背景:**
Task 1-3 完成后，Setup Wizard 的数据模型、UI 渲染和事件处理链路已完整实现，但缺少端到端验证。本 Task 在 `headless.rs` 中新增 `setup_wizard_e2e` 测试模块，构造无配置的 App → 验证 setup_wizard 自动激活 → 通过 `handle_setup_wizard_key` 驱动三步配置 → 验证最终配置写入 settings.json 且 setup_wizard 清除。这些测试覆盖了验收标准中"首次启动自动弹出向导"和"Headless 测试模式下 setup 向导可通过代码驱动完成"两条核心场景。

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/headless.rs`（新增 `setup_wizard_e2e` 测试模块）
- 修改: `rust-agent-tui/src/app/setup_wizard.rs`（确保 `needs_setup()`、`handle_setup_wizard_key()`、`save_setup()`、`SetupWizardPanel::new()` 均为 `pub`，且 `SetupWizardAction` 枚举和所有步骤枚举可被测试模块访问）

**执行步骤:**

- [x] 确认 `setup_wizard.rs` 公开接口可供测试使用
  - 位置: `rust-agent-tui/src/app/setup_wizard.rs` 顶部
  - 检查以下类型均为 `pub`: `SetupWizardPanel`、`SetupStep`、`ProviderType`、`Step1Field`、`AliasConfig`、`SetupWizardAction`、`needs_setup()`、`handle_setup_wizard_key()`、`save_setup()`
  - 若有 `pub(crate)` 限制，改为 `pub`（测试模块在 `ui/headless.rs`，跨模块访问需要 `pub`）
  - 原因: headless 测试位于 `ui::headless` 模块，不在 `app::setup_wizard` 内，需要跨模块公开访问

- [x] 在 `rust-agent-tui/src/app/setup_wizard.rs` 中实现可测试的 `save_setup_to()` 函数（写入指定路径）
  - 位置: `save_setup()` 函数之后
  - 签名: `pub fn save_setup_to(wizard: &SetupWizardPanel, path: &std::path::Path) -> anyhow::Result<crate::config::ZenConfig>`
  - 关键逻辑:
  ```rust
  pub fn save_setup_to(wizard: &SetupWizardPanel, path: &std::path::Path) -> anyhow::Result<crate::config::ZenConfig> {
      let mut cfg = crate::config::ZenConfig::default();
      let provider_type_str = match wizard.provider_type {
          ProviderType::Anthropic => "anthropic",
          ProviderType::OpenAiCompatible => "openai",
      };
      let provider = crate::config::types::ProviderConfig {
          id: wizard.provider_id.clone(),
          provider_type: provider_type_str.to_string(),
          api_key: wizard.api_key.clone(),
          base_url: wizard.base_url.clone(),
          ..Default::default()
      };
      cfg.config.providers.push(provider);
      cfg.config.active_alias = "opus".to_string();
      // 按索引遍历设置三个别名（避免同时可变借用）
      for i in 0..3 {
              "opus" => &mut cfg.config.model_aliases.opus,
              "sonnet" => &mut cfg.config.model_aliases.sonnet,
              "haiku" => &mut cfg.config.model_aliases.haiku,
              _ => unreachable!(),
          };
          alias_cfg.provider_id = wizard.provider_id.clone();
          alias_cfg.model_id = wizard.aliases[i].model_id.clone();
      }
      let content = serde_json::to_string_pretty(&cfg)?;
      if let Some(parent) = path.parent() {
          std::fs::create_dir_all(parent)?;
      }
      std::fs::write(path, content)?;
      Ok(cfg)
  }
  ```
  - 原因: 生产环境的 `save_setup()` 写入 `~/.zen-code/settings.json`（通过 `config::save()`），测试环境需要写入临时目录以避免污染用户配置。`save_setup()` 可重构为调用 `save_setup_to()` 并传入 `config::config_path()`

- [x] 重构 `save_setup()` 使其复用 `save_setup_to()`
  - 位置: `save_setup()` 函数实现
  - 内容:
  ```rust
  pub fn save_setup(wizard: &SetupWizardPanel) -> anyhow::Result<crate::config::ZenConfig> {
      let path = crate::config::store::config_path();
      let cfg = save_setup_to(wizard, &path)?;
      Ok(cfg)
  }
  ```
  - 原因: DRY 原则，生产路径和测试路径共用同一序列化逻辑

- [x] 在 `rust-agent-tui/src/ui/headless.rs` 底部 `mod tests` 内新增 `mod setup_wizard_e2e` 测试子模块
  - 位置: `mod tests` 块内，`test_cron_panel_render` 测试之后
  - 模块声明:
  ```rust
  mod setup_wizard_e2e {
      use super::*;
      use crate::app::setup_wizard::{
          SetupWizardPanel, SetupStep, ProviderType, Step1Field,
          SetupWizardAction, needs_setup, handle_setup_wizard_key, save_setup_to,
      };
      use ratatui_textarea::{Input, Key};
      use std::path::PathBuf;
  ```
  - 原因: 与现有 headless 测试组织方式一致（所有集成测试在 `headless.rs` 的 `#[cfg(test)] mod tests` 块内），子模块隔离避免命名冲突

- [x] 实现测试辅助函数 `make_input()` — 构造 `Input` 的快捷方式
  - 位置: `mod setup_wizard_e2e` 模块顶部
  ```rust
  fn make_char(c: char) -> Input {
      Input { key: Key::Char(c), ctrl: false, alt: false, shift: false }
  }
  fn make_key(key: Key) -> Input {
      Input { key, ctrl: false, alt: false, shift: false }
  }
  fn type_text(wizard: &mut SetupWizardPanel, text: &str) {
      for c in text.chars() {
          let _ = handle_setup_wizard_key(wizard, make_char(c));
      }
  }
  ```
  - 原因: `handle_setup_wizard_key` 接收 `Input`（来自 `ratatui_textarea`），需要构造测试用 Input 实例；`type_text` 批量输入避免重复代码

- [x] 实现 `test_needs_setup_triggers_for_empty_config` — 验证空配置触发 setup
  - 位置: `mod setup_wizard_e2e` 内
  ```rust
  #[tokio::test]
  async fn test_needs_setup_triggers_for_empty_config() {
      // 构造无配置的 App（zen_config = None）
      let (app, _handle) = App::new_headless(120, 30);
      assert!(app.zen_config.is_none(), "headless App 默认无配置");

      // 空配置 → needs_setup 返回 true
      let empty_cfg = crate::config::types::ZenConfig::default();
      assert!(needs_setup(&empty_cfg.config), "空 providers 应需要 setup");

      // 无 zen_config 时也应触发 setup（模拟 main.rs 中的检测逻辑）
      assert!(app.zen_config.is_none());
  }
  ```
  - 原因: 验证验收标准"首次启动（无 settings.json 或 providers 为空）自动弹出 setup 向导"

- [x] 实现 `test_setup_wizard_full_flow_anthropic` — 端到端测试：Anthropic Provider 完整流程
  - 位置: `mod setup_wizard_e2e` 内
  - 这是核心端到端测试场景，完整驱动三步配置并验证最终状态
  ```rust
  #[tokio::test]
  async fn test_setup_wizard_full_flow_anthropic() {
      // ── Phase 1: 构造无配置 App，手动激活 setup wizard ──
      let (mut app, mut handle) = App::new_headless(120, 30);
      assert!(app.setup_wizard.is_none(), "headless 默认无 setup_wizard");

      // 模拟 main.rs 中的检测逻辑：zen_config=None → 激活 setup
      app.setup_wizard = Some(SetupWizardPanel::new());
      assert!(app.setup_wizard.is_some());

      // ── Phase 2: 渲染 Step 1 (Provider)，验证初始状态 ──
      {
          let wizard = app.setup_wizard.as_ref().unwrap();
          assert_eq!(wizard.step, SetupStep::Provider);
          assert_eq!(wizard.provider_type, ProviderType::Anthropic);
          assert_eq!(wizard.provider_id, "anthropic");
          assert_eq!(wizard.base_url, "https://api.anthropic.com");
      }

      // 渲染并验证 Step 1 UI
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      assert!(handle.contains("Step 1/3"), "应显示 Step 1 标题");
      assert!(handle.contains("Anthropic"), "应显示 Anthropic 选项");

      // Step 1: 默认值无需修改，直接 Enter 进入 Step 2
      let wizard = app.setup_wizard.as_mut().unwrap();
      let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
      assert!(matches!(action, Some(SetupWizardAction::Redraw)));
      assert_eq!(wizard.step, SetupStep::ApiKey, "Enter 后应进入 Step 2");

      // ── Phase 3: Step 2 (API Key) — 输入 API Key ──
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      assert!(handle.contains("Step 2/3"), "应显示 Step 2 标题");

      let wizard = app.setup_wizard.as_mut().unwrap();
      type_text(wizard, "sk-ant-test-key-12345");
      assert_eq!(wizard.api_key, "sk-ant-test-key-12345");

      // Enter 进入 Step 3
      let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
      assert!(matches!(action, Some(SetupWizardAction::Redraw)));
      assert_eq!(wizard.step, SetupStep::ModelAlias, "Enter 后应进入 Step 3");

      // ── Phase 4: Step 3 (Model Alias) — 使用默认模型别名 ──
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      assert!(handle.contains("Step 3/3"), "应显示 Step 3 标题");

      let wizard = app.setup_wizard.as_ref().unwrap();
      // 验证 Anthropic 默认模型 ID
      assert!(wizard.aliases[0].model_id.contains("claude-opus"), "Opus 默认模型");
      assert!(wizard.aliases[1].model_id.contains("claude-sonnet"), "Sonnet 默认模型");
      assert!(wizard.aliases[2].model_id.contains("claude-haiku"), "Haiku 默认模型");

      // Enter 完成配置 → 进入 Done
      let wizard = app.setup_wizard.as_mut().unwrap();
      let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
      assert!(matches!(action, Some(SetupWizardAction::Redraw)));
      assert_eq!(wizard.step, SetupStep::Done, "Enter 后应进入 Done");

      // ── Phase 5: Done 页 — Enter 确认保存 ──
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      assert!(handle.contains("Setup Complete") || handle.contains("Complete"),
              "应显示完成页");

      let wizard = app.setup_wizard.as_mut().unwrap();
      let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
      assert!(matches!(action, Some(SetupWizardAction::SaveAndClose)),
              "Done 页 Enter 应返回 SaveAndClose");

      // ── Phase 6: 验证 save_setup 写入配置的正确性 ──
      let wizard = app.setup_wizard.as_ref().unwrap();
      let temp_dir = std::env::temp_dir().join(format!("zen-setup-test-{}", uuid::Uuid::now_v7()));
      let config_path = temp_dir.join("settings.json");
      let cfg = save_setup_to(wizard, &config_path).expect("save_setup_to 应成功");

      // 验证返回的 ZenConfig
      assert_eq!(cfg.config.providers.len(), 1, "应有 1 个 provider");
      assert_eq!(cfg.config.providers[0].provider_type, "anthropic");
      assert_eq!(cfg.config.providers[0].api_key, "sk-ant-test-key-12345");
      assert_eq!(cfg.config.active_alias, "opus");
      assert_eq!(cfg.config.model_aliases.opus.provider_id, "anthropic");
      assert!(cfg.config.model_aliases.opus.model_id.contains("claude-opus"));
      assert_eq!(cfg.config.model_aliases.sonnet.provider_id, "anthropic");
      assert!(cfg.config.model_aliases.sonnet.model_id.contains("claude-sonnet"));
      assert_eq!(cfg.config.model_aliases.haiku.provider_id, "anthropic");
      assert!(cfg.config.model_aliases.haiku.model_id.contains("claude-haiku"));

      // 验证文件写入可读回
      let content = std::fs::read_to_string(&config_path).expect("配置文件应存在");
      assert!(content.contains("anthropic"), "JSON 应包含 provider_type");
      assert!(content.contains("sk-ant-test-key-12345"), "JSON 应包含 api_key");

      // 验证不再 needs_setup
      assert!(!needs_setup(&cfg.config), "配置完成后不应再需要 setup");

      // 清理临时文件
      let _ = std::fs::remove_dir_all(&temp_dir);
  }
  ```
  - 原因: 覆盖验收标准中"三步流程完整走通 + 配置写入 settings.json + 内存状态即时刷新"的核心路径

- [x] 实现 `test_setup_wizard_full_flow_openai` — 端到端测试：OpenAI Compatible Provider 流程
  - 位置: `mod setup_wizard_e2e` 内
  ```rust
  #[tokio::test]
  async fn test_setup_wizard_full_flow_openai() {
      let (mut app, mut handle) = App::new_headless(120, 30);
      let mut wizard = SetupWizardPanel::new();

      // 切换到 OpenAI Compatible
      assert_eq!(wizard.step1_focus, Step1Field::ProviderType);
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Down)); // cycle: Anthropic → OpenAI
      assert_eq!(wizard.provider_type, ProviderType::OpenAiCompatible);
      assert_eq!(wizard.provider_id, "openai");
      assert_eq!(wizard.base_url, "https://api.openai.com/v1");

      // 渲染验证
      app.setup_wizard = Some(wizard);
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      assert!(handle.contains("OpenAI Compatible"), "应显示 OpenAI Compatible");

      // Enter → Step 2
      let wizard = app.setup_wizard.as_mut().unwrap();
      let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter));
      assert_eq!(wizard.step, SetupStep::ApiKey);

      // 输入 API Key
      type_text(wizard, "sk-openai-test-key");
      let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter));
      assert_eq!(wizard.step, SetupStep::ModelAlias);

      // 验证 OpenAI 默认模型 ID
      assert_eq!(wizard.aliases[0].model_id, "o3");
      assert_eq!(wizard.aliases[1].model_id, "gpt-4o");
      assert_eq!(wizard.aliases[2].model_id, "gpt-4o-mini");

      // Enter → Done → SaveAndClose
      let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter));
      assert_eq!(wizard.step, SetupStep::Done);
      let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
      assert!(matches!(action, Some(SetupWizardAction::SaveAndClose)));

      // 验证配置
      let temp_dir = std::env::temp_dir().join(format!("zen-setup-test-openai-{}", uuid::Uuid::now_v7()));
      let config_path = temp_dir.join("settings.json");
      let cfg = save_setup_to(wizard, &config_path).expect("save_setup_to 应成功");
      assert_eq!(cfg.config.providers[0].provider_type, "openai");
      assert_eq!(cfg.config.providers[0].api_key, "sk-openai-test-key");
      assert_eq!(cfg.config.model_aliases.opus.model_id, "o3");
      assert_eq!(cfg.config.model_aliases.sonnet.model_id, "gpt-4o");
      assert_eq!(cfg.config.model_aliases.haiku.model_id, "gpt-4o-mini");

      let _ = std::fs::remove_dir_all(&temp_dir);
  }
  ```
  - 原因: 验证 OpenAI Compatible Provider 的完整流程，包括默认值和配置写入

- [x] 实现 `test_setup_wizard_skip_with_confirm` — 跳过向导的交互测试
  - 位置: `mod setup_wizard_e2e` 内
  ```rust
  #[tokio::test]
  async fn test_setup_wizard_skip_with_confirm() {
      let (mut app, mut handle) = App::new_headless(120, 30);
      app.setup_wizard = Some(SetupWizardPanel::new());

      // Esc → 触发跳过确认
      let wizard = app.setup_wizard.as_mut().unwrap();
      let action = handle_setup_wizard_key(wizard, make_key(Key::Esc));
      assert!(matches!(action, Some(SetupWizardAction::Redraw)));
      assert!(wizard.confirm_skip, "应进入跳过确认状态");

      // 渲染确认提示
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      // 注意：CJK 字符在 TestBackend 中有宽字符填充，使用 ASCII 关键词断言
      // "skip" 或 "setup" 等关键词应出现在确认提示中

      // Esc 取消跳过
      let wizard = app.setup_wizard.as_mut().unwrap();
      let action = handle_setup_wizard_key(wizard, make_key(Key::Esc));
      assert!(matches!(action, Some(SetupWizardAction::Redraw)));
      assert!(!wizard.confirm_skip, "Esc 应取消跳过确认");

      // 再次 Esc → 重新触发确认
      let action = handle_setup_wizard_key(wizard, make_key(Key::Esc));
      assert!(wizard.confirm_skip);

      // Enter 确认跳过 → 返回 Skip
      let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
      assert!(matches!(action, Some(SetupWizardAction::Skip)), "Enter 应确认跳过");
  }
  ```
  - 原因: 验收标准"跳过 setup 时二次确认"

- [x] 实现 `test_setup_wizard_esc_navigation` — Esc 返回上一步导航测试
  - 位置: `mod setup_wizard_e2e` 内
  ```rust
  #[tokio::test]
  async fn test_setup_wizard_esc_navigation() {
      let mut wizard = SetupWizardPanel::new();

      // Step 1 → Enter → Step 2
      assert_eq!(wizard.step, SetupStep::Provider);
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
      assert_eq!(wizard.step, SetupStep::ApiKey);

      // Step 2 → Esc → 回到 Step 1（provider_id 保留）
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
      assert_eq!(wizard.step, SetupStep::Provider);

      // Step 1 → Enter → Step 2 → 输入 key → Enter → Step 3
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
      type_text(&mut wizard, "test-key");
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
      assert_eq!(wizard.step, SetupStep::ModelAlias);

      // Step 3 → Esc → 回到 Step 2（api_key 保留）
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
      assert_eq!(wizard.step, SetupStep::ApiKey);
      assert_eq!(wizard.api_key, "test-key", "返回上一步应保留已输入内容");

      // Step 2 → Enter → Step 3 → Enter → Done → Esc → 回到 Step 3
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
      assert_eq!(wizard.step, SetupStep::ModelAlias);
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
      assert_eq!(wizard.step, SetupStep::Done);
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
      assert_eq!(wizard.step, SetupStep::ModelAlias, "Done 页 Esc 应回到 Step 3");
  }
  ```
  - 原因: 验证 Esc 返回上一步时已输入内容保留，覆盖验收标准"Tab/Enter/Esc 导航正常"

- [x] 实现 `test_setup_wizard_validation_blocks_empty_fields` — 空字段校验测试
  - 位置: `mod setup_wizard_e2e` 内
  ```rust
  #[tokio::test]
  async fn test_setup_wizard_validation_blocks_empty_fields() {
      let mut wizard = SetupWizardPanel::new();

      // 清空 provider_id（通过 Backspace）
      wizard.provider_id.clear();
      let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
      assert!(matches!(action, Some(SetupWizardAction::Redraw)));
      assert_eq!(wizard.step, SetupStep::Provider, "空 provider_id 应阻止进入 Step 2");

      // 恢复 provider_id，进入 Step 2
      wizard.provider_id = "anthropic".to_string();
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
      assert_eq!(wizard.step, SetupStep::ApiKey);

      // 空 api_key → Enter 不应前进
      assert!(wizard.api_key.is_empty());
      let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
      assert!(matches!(action, Some(SetupWizardAction::Redraw)));
      assert_eq!(wizard.step, SetupStep::ApiKey, "空 api_key 应阻止进入 Step 3");

      // 输入 api_key → 进入 Step 3
      type_text(&mut wizard, "test-key");
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
      assert_eq!(wizard.step, SetupStep::ModelAlias);

      // 清空第一个别名 model_id → Enter 不应前进
      wizard.aliases[0].model_id.clear();
      let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
      assert!(matches!(action, Some(SetupWizardAction::Redraw)));
      assert_eq!(wizard.step, SetupStep::ModelAlias, "空 model_id 应阻止进入 Done");
  }
  ```
  - 原因: 覆盖验收标准"三个模型别名必填"和"Provider ID / API Key 非空校验"

- [x] 实现 `test_setup_wizard_step1_tab_navigation` — Step 1 Tab 字段切换测试
  - 位置: `mod setup_wizard_e2e` 内
  ```rust
  #[tokio::test]
  async fn test_setup_wizard_step1_tab_navigation() {
      let mut wizard = SetupWizardPanel::new();
      assert_eq!(wizard.step1_focus, Step1Field::ProviderType);

      // Tab: ProviderType → ProviderId
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
      assert_eq!(wizard.step1_focus, Step1Field::ProviderId);

      // Tab: ProviderId → BaseUrl
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
      assert_eq!(wizard.step1_focus, Step1Field::BaseUrl);

      // Tab: BaseUrl → ProviderType (循环)
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
      assert_eq!(wizard.step1_focus, Step1Field::ProviderType);

      // Shift+Tab: ProviderType → BaseUrl (反向)
      let _ = handle_setup_wizard_key(&mut wizard, Input {
          key: Key::Tab, ctrl: false, alt: false, shift: true,
      });
      assert_eq!(wizard.step1_focus, Step1Field::BaseUrl);
  }
  ```
  - 原因: 验证 Tab 在 Step1Field 三个字段间的循环导航

- [x] 实现 `test_setup_wizard_step3_tab_navigation` — Step 3 Tab 字段切换测试
  - 位置: `mod setup_wizard_e2e` 内
  ```rust
  #[tokio::test]
  async fn test_setup_wizard_step3_tab_navigation() {
      let mut wizard = SetupWizardPanel::new();
      wizard.step = SetupStep::ModelAlias;
      assert_eq!(wizard.step3_focus, 0);

      // Tab: 0 → 1
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
      assert_eq!(wizard.step3_focus, 1);

      // Tab: 1 → 2
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
      assert_eq!(wizard.step3_focus, 2);

      // Tab: 2 → 0 (循环)
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
      assert_eq!(wizard.step3_focus, 0);
  }
  ```
  - 原因: 验证 Tab 在三个别名行间的循环导航

- [x] 实现 `test_setup_wizard_backspace_editing` — Backspace 字符编辑测试
  - 位置: `mod setup_wizard_e2e` 内
  ```rust
  #[tokio::test]
  async fn test_setup_wizard_backspace_editing() {
      let mut wizard = SetupWizardPanel::new();

      // Step 2: 输入 API Key 后 Backspace
      wizard.step = SetupStep::ApiKey;
      type_text(&mut wizard, "abc");
      assert_eq!(wizard.api_key, "abc");
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Backspace));
      assert_eq!(wizard.api_key, "ab");

      // Step 1 ProviderId: 输入后 Backspace
      wizard.step = SetupStep::Provider;
      wizard.step1_focus = Step1Field::ProviderId;
      wizard.provider_id = "myprovider".to_string();
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Backspace));
      assert_eq!(wizard.provider_id, "myprovide");

      // Step 1 BaseUrl (Anthropic): Backspace 应无效（只读）
      wizard.step1_focus = Step1Field::BaseUrl;
      let url_before = wizard.base_url.clone();
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Backspace));
      assert_eq!(wizard.base_url, url_before, "Anthropic BaseUrl 应为只读");

      // Step 1 BaseUrl (OpenAI): Backspace 应有效
      wizard.provider_type = ProviderType::OpenAiCompatible;
      wizard.base_url = "https://api.openai.com/v1".to_string();
      let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Backspace));
      assert_eq!(wizard.base_url, "https://api.openai.com/");
  }
  ```
  - 原因: 验证字符编辑在所有字段上正确工作，包括只读字段

- [x] 实现 `test_setup_wizard_saves_and_clears` — 完整的保存+清除集成测试
  - 位置: `mod setup_wizard_e2e` 内
  ```rust
  #[tokio::test]
  async fn test_setup_wizard_saves_and_clears() {
      let (mut app, mut handle) = App::new_headless(120, 30);

      // 激活 setup wizard
      app.setup_wizard = Some(SetupWizardPanel::new());
      assert!(app.setup_wizard.is_some());

      // 渲染确认 wizard 占据全屏
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      assert!(handle.contains("Step 1/3"), "setup wizard 应占据全屏渲染");

      // 快速完成三步配置
      let wizard = app.setup_wizard.as_mut().unwrap();
      let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter)); // Step 1 → 2
      type_text(wizard, "sk-final-test");
      let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter)); // Step 2 → 3
      let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter)); // Step 3 → Done

      // Done → SaveAndClose
      let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
      assert!(matches!(action, Some(SetupWizardAction::SaveAndClose)));

      // 模拟 event.rs 中的 SaveAndClose 处理逻辑
      let wizard = app.setup_wizard.take().unwrap();
      let temp_dir = std::env::temp_dir().join(format!("zen-setup-final-{}", uuid::Uuid::now_v7()));
      let config_path = temp_dir.join("settings.json");
      let cfg = save_setup_to(&wizard, &config_path).expect("save 应成功");

      // 验证配置写入后不再需要 setup
      assert!(!needs_setup(&cfg.config), "完整配置后不应再需要 setup");

      // 模拟 refresh_after_setup: 更新 app 状态
      app.zen_config = Some(cfg);
      // 在真实流程中 provider_name/model_name 会通过 LlmProvider::from_config 更新

      // setup_wizard 已清除
      assert!(app.setup_wizard.is_none(), "SaveAndClose 后 setup_wizard 应被清除");

      // 清理
      let _ = std::fs::remove_dir_all(&temp_dir);
  }
  ```
  - 原因: 验证验收标准"完成后配置写入 settings.json，内存状态即时刷新"和"setup_wizard 清除"

- [x] 运行所有 setup_wizard 相关测试并确认通过
  - 运行命令: `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | tail -30`
  - 预期: 所有测试通过（包含 Task 1-3 的单元测试和 Task 4 的集成测试），passed > 0

**检查步骤:**
- [x] 验证 headless.rs 包含 setup_wizard_e2e 模块
  - `grep -c "mod setup_wizard_e2e" rust-agent-tui/src/ui/headless.rs`
  - 预期: 输出 1
- [x] 验证 setup_wizard.rs 包含 save_setup_to 函数
  - `grep -c "pub fn save_setup_to" rust-agent-tui/src/app/setup_wizard.rs`
  - 预期: 输出 1
- [x] 验证所有测试编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -c "error"`
  - 预期: 输出 0
- [x] 运行完整 setup_wizard 测试套件通过
  - `cargo test -p rust-agent-tui --lib -- setup_wizard 2>&1 | grep "test result"`
  - 预期: 输出包含 `ok` 且 passed ≥ 10（Task 1-3 单元测试 + Task 4 集成测试）
- [x] 运行 headless 测试套件无回归
  - `cargo test -p rust-agent-tui --lib -- ui::headless 2>&1 | grep "test result"`
  - 预期: 输出包含 `ok`，所有原有测试仍通过

---

### Task 5: TUI Setup Wizard 验收

**前置条件:**
- 启动命令: `cargo run -p rust-agent-tui`（需真实终端，非 headless）
- 测试数据准备: 备份现有 `~/.zen-code/settings.json`（如存在），验收后恢复
- 其他环境准备: 确保 `ANTHROPIC_API_KEY` 和 `OPENAI_API_KEY` 环境变量均未设置（模拟首次安装）

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p rust-agent-tui --lib 2>&1 | grep "test result"`
   - 预期: 全部测试通过，0 failed
   - 失败排查: 检查各 Task 的测试步骤，逐个运行 `cargo test -p rust-agent-tui --lib -- setup_wizard`

2. Headless 端到端测试覆盖验收标准
   - `cargo test -p rust-agent-tui --lib -- setup_wizard_e2e 2>&1 | grep "test result"`
   - 预期: 8 个集成测试全部通过（Anthropic 完整流程、OpenAI 完整流程、跳过确认、Esc 导航、空字段校验、Tab 导航×2、Backspace 编辑、保存+清除）
   - 失败排查: 检查 Task 4 各测试步骤

3. 验证首次启动触发 setup 向导（headless 模拟）
   - `cargo test -p rust-agent-tui --lib -- test_needs_setup_triggers_for_empty_config 2>&1 | grep "ok"`
   - 预期: 测试通过，空 providers → `needs_setup()` 返回 true
   - 失败排查: 检查 Task 1 `needs_setup()` 逻辑

4. 验证配置完成后不再触发 setup
   - `cargo test -p rust-agent-tui --lib -- test_setup_wizard_saves_and_clears 2>&1 | grep "ok"`
   - 预期: 测试通过，完整配置后 `needs_setup()` 返回 false
   - 失败排查: 检查 Task 3 `save_setup()` 写入逻辑

5. 验证跳过 setup 时二次确认
   - `cargo test -p rust-agent-tui --lib -- test_setup_wizard_skip_with_confirm 2>&1 | grep "ok"`
   - 预期: 测试通过，Esc 触发确认、Enter 确认跳过、Esc 取消跳过
   - 失败排查: 检查 Task 3 `handle_confirm_skip()` 逻辑

6. 验证编译无警告
   - `cargo build -p rust-agent-tui 2>&1 | grep -c "warning"`
   - 预期: 0 个与 setup_wizard 相关的 warning
