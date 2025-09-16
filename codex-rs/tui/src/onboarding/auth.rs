use codex_login::CLIENT_ID;
use codex_login::ServerOptions;
use codex_login::ShutdownHandle;
use codex_login::run_login_server;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;

use codex_login::AuthMode;
use codex_core::claude_auth::{ClaudeAuthMode, ClaudeAuth, SubscriptionInfo};
use codex_core::unified_auth::{UnifiedAuthManager, AuthProvider as UnifiedAuthProvider};

use codex_core::config::GPT_5_CODEX_MEDIUM_MODEL;
use codex_core::model_family::{derive_default_model_family, find_family_for_model};

use crate::LoginStatus;
use crate::app::ChatWidgetArgs;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::onboarding::onboarding_screen::KeyboardHandler;
use crate::onboarding::onboarding_screen::StepStateProvider;
use crate::shimmer::shimmer_spans;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use super::onboarding_screen::StepState;
// no additional imports

#[derive(Debug)]
pub(crate) enum SignInState {
    PickProvider,
    PickMode(SignInProvider),
    ChatGptContinueInBrowser(ContinueInBrowserState),
    ChatGptSuccessMessage,
    ChatGptSuccess,
    ClaudeContinueInBrowser(ClaudeAuthState),
    ClaudeSuccessMessage,
    ClaudeSuccess,
    EnvVarMissing,
    EnvVarFound,
}

/// Authentication provider selection
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SignInProvider {
    OpenAI(OpenAIAuthState),
    Claude(ClaudeAuthState),
}

/// OpenAI authentication state
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenAIAuthState {
    pub mode: AuthMode,
}

/// Claude authentication state
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClaudeAuthState {
    pub auth_mode: ClaudeAuthMode,
    pub subscription_status: Option<String>,
    pub verification_url: Option<String>,
    pub subscription_info: Option<SubscriptionInfo>,
}

#[derive(Debug)]
/// Used to manage the lifecycle of SpawnedLogin and ensure it gets cleaned up.
pub(crate) struct ContinueInBrowserState {
    auth_url: String,
    shutdown_handle: Option<ShutdownHandle>,
    _login_wait_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for ContinueInBrowserState {
    fn drop(&mut self) {
        if let Some(flag) = &self.shutdown_handle {
            flag.shutdown();
        }
    }
}

impl KeyboardHandler for AuthModeWidget {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.handle_up_navigation();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.handle_down_navigation();
            }
            KeyCode::Char('1') => {
                self.handle_option_1();
            }
            KeyCode::Char('2') => {
                self.handle_option_2();
            }
            KeyCode::Char('3') => {
                self.handle_option_3();
            }
            KeyCode::Enter => {
                self.handle_enter();
            }
            KeyCode::Esc => {
                self.handle_escape();
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthModeWidget {
    pub event_tx: AppEventSender,
    pub highlighted_provider: SignInProvider,
    pub highlighted_mode: AuthMode,
    pub error: Option<String>,
    pub sign_in_state: SignInState,
    pub codex_home: PathBuf,
    pub login_status: LoginStatus,
    pub preferred_auth_method: AuthMode,
    pub unified_auth: Option<UnifiedAuthManager>,
    pub chat_widget_args: Arc<Mutex<ChatWidgetArgs>>,
}

impl AuthModeWidget {
    /// Handle up navigation based on current state
    fn handle_up_navigation(&mut self) {
        match &self.sign_in_state {
            SignInState::PickProvider => {
                match &self.highlighted_provider {
                    SignInProvider::Claude(_) => {
                        self.highlighted_provider = SignInProvider::OpenAI(OpenAIAuthState {
                            mode: AuthMode::ChatGPT,
                        });
                    }
                    SignInProvider::OpenAI(_) => {
                        self.highlighted_provider = SignInProvider::Claude(ClaudeAuthState {
                            auth_mode: ClaudeAuthMode::MaxSubscription,
                            subscription_status: None,
                            verification_url: None,
                            subscription_info: None,
                        });
                    }
                }
            }
            SignInState::PickMode(provider) => {
                match provider {
                    SignInProvider::OpenAI(_) => {
                        self.highlighted_mode = AuthMode::ChatGPT;
                    }
                    SignInProvider::Claude(_) => {
                        // Claude navigation between subscription types
                        // For now, keep it simple with the current highlighted provider
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle down navigation based on current state
    fn handle_down_navigation(&mut self) {
        match &self.sign_in_state {
            SignInState::PickProvider => {
                match &self.highlighted_provider {
                    SignInProvider::OpenAI(_) => {
                        self.highlighted_provider = SignInProvider::Claude(ClaudeAuthState {
                            auth_mode: ClaudeAuthMode::MaxSubscription,
                            subscription_status: None,
                            verification_url: None,
                            subscription_info: None,
                        });
                    }
                    SignInProvider::Claude(_) => {
                        self.highlighted_provider = SignInProvider::OpenAI(OpenAIAuthState {
                            mode: AuthMode::ChatGPT,
                        });
                    }
                }
            }
            SignInState::PickMode(provider) => {
                match provider {
                    SignInProvider::OpenAI(_) => {
                        self.highlighted_mode = AuthMode::ApiKey;
                    }
                    SignInProvider::Claude(_) => {
                        // Claude navigation between subscription types
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle option 1 (first choice)
    fn handle_option_1(&mut self) {
        match &self.sign_in_state {
            SignInState::PickProvider => {
                // Select OpenAI provider
                let openai_state = OpenAIAuthState { mode: AuthMode::ChatGPT };
                self.highlighted_provider = SignInProvider::OpenAI(openai_state.clone());
                self.sign_in_state = SignInState::PickMode(SignInProvider::OpenAI(openai_state));
            }
            SignInState::PickMode(SignInProvider::OpenAI(_)) => {
                self.start_chatgpt_login();
            }
            SignInState::PickMode(SignInProvider::Claude(_)) => {
                self.start_claude_auth();
            }
            _ => {}
        }
    }

    /// Handle option 2 (second choice)
    fn handle_option_2(&mut self) {
        match &self.sign_in_state {
            SignInState::PickProvider => {
                // Select Claude provider
                let claude_state = ClaudeAuthState {
                    auth_mode: ClaudeAuthMode::MaxSubscription,
                    subscription_status: None,
                    verification_url: None,
                    subscription_info: None,
                };
                self.highlighted_provider = SignInProvider::Claude(claude_state.clone());
                self.sign_in_state = SignInState::PickMode(SignInProvider::Claude(claude_state));
            }
            SignInState::PickMode(SignInProvider::OpenAI(_)) => {
                self.verify_api_key();
            }
            SignInState::PickMode(SignInProvider::Claude(_)) => {
                self.verify_claude_api_key();
            }
            _ => {}
        }
    }

    /// Handle option 3 (third choice, if available)
    fn handle_option_3(&mut self) {
        // Reserved for future use or going back
        match &self.sign_in_state {
            SignInState::PickMode(_) => {
                // Go back to provider selection
                self.sign_in_state = SignInState::PickProvider;
            }
            _ => {}
        }
    }

    /// Handle Enter key
    fn handle_enter(&mut self) {
        match &self.sign_in_state {
            SignInState::PickProvider => {
                // Enter provider selection
                match &self.highlighted_provider {
                    SignInProvider::OpenAI(state) => {
                        self.sign_in_state = SignInState::PickMode(SignInProvider::OpenAI(state.clone()));
                    }
                    SignInProvider::Claude(state) => {
                        self.sign_in_state = SignInState::PickMode(SignInProvider::Claude(state.clone()));
                    }
                }
            }
            SignInState::PickMode(provider) => {
                match provider {
                    SignInProvider::OpenAI(_) => {
                        match self.highlighted_mode {
                            AuthMode::ChatGPT => self.start_chatgpt_login(),
                            AuthMode::ApiKey => self.verify_api_key(),
                        }
                    }
                    SignInProvider::Claude(_) => {
                        self.start_claude_auth();
                    }
                }
            }
            SignInState::EnvVarMissing => {
                self.sign_in_state = SignInState::PickProvider;
            }
            SignInState::ChatGptSuccessMessage => {
                self.sign_in_state = SignInState::ChatGptSuccess;
            }
            SignInState::ClaudeSuccessMessage => {
                self.sign_in_state = SignInState::ClaudeSuccess;
            }
            _ => {}
        }
    }

    /// Handle Escape key
    fn handle_escape(&mut self) {
        match &self.sign_in_state {
            SignInState::ChatGptContinueInBrowser(_) => {
                self.sign_in_state = SignInState::PickProvider;
            }
            SignInState::ClaudeContinueInBrowser(_) => {
                self.sign_in_state = SignInState::PickProvider;
            }
            SignInState::PickMode(_) => {
                self.sign_in_state = SignInState::PickProvider;
            }
            _ => {}
        }
    }

    /// Start Claude authentication process
    fn start_claude_auth(&mut self) {
        // TODO: Implement Claude OAuth flow similar to ChatGPT
        // For now, show a placeholder state
        let claude_state = ClaudeAuthState {
            auth_mode: ClaudeAuthMode::MaxSubscription,
            subscription_status: Some("Checking subscription...".to_string()),
            verification_url: Some("https://auth.anthropic.com/oauth/authorize".to_string()),
            subscription_info: None,
        };
        
        self.sign_in_state = SignInState::ClaudeContinueInBrowser(claude_state);
        self.event_tx.send(AppEvent::RequestRedraw);
    }

    /// Verify Claude API key from environment
    fn verify_claude_api_key(&mut self) {
        // Check for Claude API key in environment variables
        if let Some(_api_key) = codex_core::claude_auth::read_claude_api_key_from_env() {
            self.sign_in_state = SignInState::ClaudeSuccess;
        } else {
            self.sign_in_state = SignInState::EnvVarMissing;
        }
        self.event_tx.send(AppEvent::RequestRedraw);
    }

    fn render_pick_provider(&self, area: Rect, buf: &mut Buffer) {
        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::raw("> "),
                Span::styled(
                    "Choose your AI provider",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "Select between OpenAI and Claude for your coding assistant",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
        ];

        let create_provider_item = |idx: usize,
                                   selected_provider: &SignInProvider,
                                   provider_name: &str,
                                   description: &str,
                                   features: &[&str]|
         -> Vec<Line<'static>> {
            let is_selected = match (&self.highlighted_provider, selected_provider) {
                (SignInProvider::OpenAI(_), SignInProvider::OpenAI(_)) => true,
                (SignInProvider::Claude(_), SignInProvider::Claude(_)) => true,
                _ => false,
            };
            let caret = if is_selected { ">" } else { " " };

            let line1 = if is_selected {
                Line::from(vec![
                    format!("{} {}. ", caret, idx + 1)
                        .fg(crate::colors::info())
                        .dim(),
                    provider_name.to_string().fg(crate::colors::info()),
                ])
            } else {
                Line::from(format!("  {}. {}", idx + 1, provider_name))
                    .style(Style::default().fg(crate::colors::text()))
            };

            let line2 = if is_selected {
                Line::from(format!("     {}", description))
                    .fg(crate::colors::info())
                    .add_modifier(Modifier::DIM)
            } else {
                Line::from(format!("     {}", description))
                    .style(Style::default().fg(crate::colors::text_dim()))
            };

            let mut result = vec![line1, line2];
            
            // Add features if selected
            if is_selected {
                for feature in features {
                    result.push(
                        Line::from(format!("     • {}", feature))
                            .fg(crate::colors::info())
                            .add_modifier(Modifier::DIM)
                    );
                }
            }

            result
        };

        lines.extend(create_provider_item(
            0,
            &SignInProvider::OpenAI(OpenAIAuthState { mode: AuthMode::ChatGPT }),
            "OpenAI (ChatGPT & GPT models)",
            "Well-established AI provider with ChatGPT Plus/Pro plans",
            &["ChatGPT subscription support", "GPT-4 and o-series models", "Large ecosystem"],
        ));

        lines.push(Line::from(""));

        lines.extend(create_provider_item(
            1,
            &SignInProvider::Claude(ClaudeAuthState {
                auth_mode: ClaudeAuthMode::MaxSubscription,
                subscription_status: None,
                verification_url: None,
                subscription_info: None,
            }),
            "Claude (Anthropic)",
            "Advanced AI assistant with Claude Max subscription support",
            &["Claude Max subscription support", "Claude 3.5 Sonnet and newer models", "Better code understanding"],
        ));

        lines.push(Line::from(""));
        lines.push(
            Line::from("  Press Enter to continue or use number keys (1-2)")
                .style(Style::default().fg(crate::colors::text_dim())),
        );

        if let Some(err) = &self.error {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                err.as_str(),
                Style::default().fg(crate::colors::error()),
            )));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_pick_mode(&self, area: Rect, buf: &mut Buffer) {
        if let SignInState::PickMode(provider) = &self.sign_in_state {
            match provider {
                SignInProvider::OpenAI(_) => {
                    self.render_openai_mode_selection(area, buf);
                }
                SignInProvider::Claude(_) => {
                    self.render_claude_mode_selection(area, buf);
                }
            }
        }
    }

    fn render_openai_mode_selection(&self, area: Rect, buf: &mut Buffer) {
        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::raw("> "),
                Span::styled(
                    "Sign in with ChatGPT to use your paid OpenAI plan",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "or connect an API key for usage-based billing",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
        ];

        // If the user is already authenticated but the method differs from their
        // preferred auth method, show a brief explanation.
        if let LoginStatus::AuthMode(current) = self.login_status {
            if current != self.preferred_auth_method {
                let to_label = |mode: AuthMode| match mode {
                    AuthMode::ApiKey => "API key",
                    AuthMode::ChatGPT => "ChatGPT",
                };
                let msg = format!(
                    "  You’re currently using {} while your preferred method is {}.",
                    to_label(current),
                    to_label(self.preferred_auth_method)
                );
                lines.push(
                    Line::from(msg)
                        .style(Style::default().fg(crate::colors::text_dim())),
                );
                lines.push(Line::from(""));
            }
        }

        let create_mode_item = |idx: usize,
                                selected_mode: AuthMode,
                                text: &str,
                                description: &str|
         -> Vec<Line<'static>> {
            let is_selected = self.highlighted_mode == selected_mode;
            let caret = if is_selected { ">" } else { " " };

            let line1 = if is_selected {
                Line::from(vec![
                    format!("{} {}. ", caret, idx + 1)
                        .fg(crate::colors::info())
                        .dim(),
                    text.to_string().fg(crate::colors::info()),
                ])
            } else {
                Line::from(format!("  {}. {text}", idx + 1))
                    .style(Style::default().fg(crate::colors::text()))
            };

            let line2 = if is_selected {
                Line::from(format!("     {description}"))
                    .fg(crate::colors::info())
                    .add_modifier(Modifier::DIM)
            } else {
                Line::from(format!("     {description}"))
                    .style(Style::default().fg(crate::colors::text_dim()))
            };

            vec![line1, line2]
        };
        let chatgpt_label = if matches!(self.login_status, LoginStatus::AuthMode(AuthMode::ChatGPT))
        {
            "Continue using ChatGPT"
        } else {
            "Sign in with ChatGPT"
        };

        lines.extend(create_mode_item(
            0,
            AuthMode::ChatGPT,
            chatgpt_label,
            "Usage included with Plus, Pro, and Team plans",
        ));
        let api_key_label = if matches!(self.login_status, LoginStatus::AuthMode(AuthMode::ApiKey))
        {
            "Continue using API key"
        } else {
            "Provide your own API key"
        };
        lines.extend(create_mode_item(
            1,
            AuthMode::ApiKey,
            api_key_label,
            "Pay for what you use",
        ));
        lines.push(Line::from(""));
        lines.push(
            Line::from("  Press Enter to continue or press 3 to go back")
                .style(Style::default().fg(crate::colors::text_dim())),
        );
        if let Some(err) = &self.error {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                err.as_str(),
                Style::default().fg(crate::colors::error()),
            )));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_claude_mode_selection(&self, area: Rect, buf: &mut Buffer) {
        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::raw("> "),
                Span::styled(
                    "Sign in with Claude to use your Anthropic subscription",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "or connect an API key for usage-based billing",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
        ];

        let create_mode_item = |idx: usize,
                                text: &str,
                                description: &str|
         -> Vec<Line<'static>> {
            // For Claude, we'll highlight the first option by default
            let is_selected = idx == 0;
            let caret = if is_selected { ">" } else { " " };

            let line1 = if is_selected {
                Line::from(vec![
                    format!("{} {}. ", caret, idx + 1)
                        .fg(crate::colors::info())
                        .dim(),
                    text.to_string().fg(crate::colors::info()),
                ])
            } else {
                Line::from(format!("  {}. {text}", idx + 1))
                    .style(Style::default().fg(crate::colors::text()))
            };

            let line2 = if is_selected {
                Line::from(format!("     {description}"))
                    .fg(crate::colors::info())
                    .add_modifier(Modifier::DIM)
            } else {
                Line::from(format!("     {description}"))
                    .style(Style::default().fg(crate::colors::text_dim()))
            };

            vec![line1, line2]
        };

        lines.extend(create_mode_item(
            0,
            "Sign in with Claude Max/Pro",
            "Use your Claude subscription for unlimited usage"
        ));

        lines.extend(create_mode_item(
            1,
            "Provide your Claude API key",
            "Pay for what you use with your Anthropic API key"
        ));

        lines.push(Line::from(""));
        lines.push(
            Line::from("  Press Enter to continue or press 3 to go back")
                .style(Style::default().fg(crate::colors::text_dim())),
        );

        if let Some(err) = &self.error {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                err.as_str(),
                Style::default().fg(crate::colors::error()),
            )));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_continue_in_browser(&self, area: Rect, buf: &mut Buffer) {
        let mut spans = vec![Span::from("> ")];
        // Schedule a follow-up frame to keep the shimmer animation going.
        self.event_tx
            .send(AppEvent::ScheduleFrameIn(std::time::Duration::from_millis(
                100,
            )));
        spans.extend(shimmer_spans("Finish signing in via your browser"));
        let mut lines = vec![Line::from(spans), Line::from("")];
        if let SignInState::ChatGptContinueInBrowser(state) = &self.sign_in_state {
            if !state.auth_url.is_empty() {
                lines.push(Line::from("  If the link doesn't open automatically, open the following link to authenticate:"));
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    state.auth_url
                        .as_str()
                        .fg(crate::colors::info())
                        .underlined(),
                ]));
                lines.push(Line::from(""));
            }
        }

        lines.push(
            Line::from("  Press Esc to cancel").style(Style::default().add_modifier(Modifier::DIM)),
        );
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_chatgpt_success_message(&self, area: Rect, buf: &mut Buffer) {
        let lines = vec![
            Line::from("✓ Signed in with your ChatGPT account")
                .fg(crate::colors::success()),
            Line::from(""),
            Line::from("> Before you start:"),
            Line::from(""),
            Line::from("  Decide how much autonomy you want to grant Code"),
            Line::from(vec![
                Span::raw("  For more details see the "),
                Span::styled(
                    "\u{1b}]8;;https://github.com/just-every/code\u{7}Code docs\u{1b}]8;;\u{7}",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
            ])
            .style(Style::default().add_modifier(Modifier::DIM)),
            Line::from(""),
            Line::from("  Code can make mistakes"),
            Line::from("  Review the code it writes and commands it runs")
                .style(Style::default().add_modifier(Modifier::DIM)),
            Line::from(""),
            Line::from("  Powered by your ChatGPT account"),
            Line::from(vec![
                Span::raw("  Uses your plan's rate limits and "),
                Span::styled(
                    "\u{1b}]8;;https://chatgpt.com/#settings\u{7}training data preferences\u{1b}]8;;\u{7}",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
            ])
            .style(Style::default().add_modifier(Modifier::DIM)),
            Line::from(""),
            Line::from("  Press Enter to continue").fg(crate::colors::info()),
        ];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_chatgpt_success(&self, area: Rect, buf: &mut Buffer) {
        let lines = vec![Line::from("✓ Signed in with your ChatGPT account").fg(crate::colors::success())];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_env_var_found(&self, area: Rect, buf: &mut Buffer) {
        let lines = vec![Line::from("✓ Using OPENAI_API_KEY").fg(crate::colors::success())];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_claude_continue_in_browser(&self, area: Rect, buf: &mut Buffer, claude_state: &ClaudeAuthState) {
        let mut spans = vec![Span::from("> ")];
        // Schedule a follow-up frame to keep the shimmer animation going.
        self.event_tx
            .send(AppEvent::ScheduleFrameIn(std::time::Duration::from_millis(
                100,
            )));
        spans.extend(shimmer_spans("Finish signing in with Claude via your browser"));
        let mut lines = vec![Line::from(spans), Line::from("")];
        
        if let Some(verification_url) = &claude_state.verification_url {
            if !verification_url.is_empty() {
                lines.push(Line::from("  If the link doesn't open automatically, open the following link to authenticate:"));
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    verification_url
                        .as_str()
                        .fg(crate::colors::info())
                        .underlined(),
                ]));
                lines.push(Line::from(""));
            }
        }

        if let Some(status) = &claude_state.subscription_status {
            lines.push(Line::from(format!("  Status: {}", status)));
        }

        lines.push(
            Line::from("  Press Esc to cancel").style(Style::default().add_modifier(Modifier::DIM)),
        );
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_claude_success_message(&self, area: Rect, buf: &mut Buffer) {
        let lines = vec![
            Line::from("✓ Signed in with your Claude account")
                .fg(crate::colors::success()),
            Line::from(""),
            Line::from("> Before you start:"),
            Line::from(""),
            Line::from("  Decide how much autonomy you want to grant Code"),
            Line::from(vec![
                Span::raw("  For more details see the "),
                Span::styled(
                    "\u{1b}]8;;https://github.com/just-every/code\u{7}Code docs\u{1b}]8;;\u{7}",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
            ])
            .style(Style::default().add_modifier(Modifier::DIM)),
            Line::from(""),
            Line::from("  Code can make mistakes"),
            Line::from("  Review the code it writes and commands it runs")
                .style(Style::default().add_modifier(Modifier::DIM)),
            Line::from(""),
            Line::from("  Powered by your Claude subscription"),
            Line::from("  Uses your plan's rate limits and privacy settings")
                .style(Style::default().add_modifier(Modifier::DIM)),
            Line::from(""),
            Line::from("  Press Enter to continue").fg(crate::colors::info()),
        ];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_claude_success(&self, area: Rect, buf: &mut Buffer) {
        let lines = vec![Line::from("✓ Signed in with your Claude account").fg(crate::colors::success())];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_env_var_missing(&self, area: Rect, buf: &mut Buffer) {
        let lines = vec![
            Line::from(
                "  To use Code with the API, set OPENAI_API_KEY or ANTHROPIC_API_KEY in your environment",
            )
            .style(Style::default().fg(crate::colors::info())),
            Line::from(""),
            Line::from("  Press Enter to return")
                .style(Style::default().add_modifier(Modifier::DIM)),
        ];

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn start_chatgpt_login(&mut self) {
        // If we're already authenticated with ChatGPT, don't start a new login –
        // just proceed to the success message flow.
        if matches!(self.login_status, LoginStatus::AuthMode(AuthMode::ChatGPT)) {
            self.apply_chatgpt_login_side_effects();
            self.sign_in_state = SignInState::ChatGptSuccess;
            self.event_tx.send(AppEvent::RequestRedraw);
            return;
        }

        self.error = None;
        let opts = ServerOptions::new(
            self.codex_home.clone(),
            CLIENT_ID.to_string(),
            codex_core::default_client::DEFAULT_ORIGINATOR.to_string(),
        );
        let server = run_login_server(opts);
        match server {
            Ok(child) => {
                let auth_url = child.auth_url.clone();
                let shutdown_handle = child.cancel_handle();

                let event_tx = self.event_tx.clone();
                let join_handle = tokio::spawn(async move {
                    spawn_completion_poller(child, event_tx).await;
                });
                self.sign_in_state =
                    SignInState::ChatGptContinueInBrowser(ContinueInBrowserState {
                        auth_url,
                        shutdown_handle: Some(shutdown_handle),
                        _login_wait_handle: Some(join_handle),
                    });
                self.event_tx.send(AppEvent::RequestRedraw);
            }
            Err(e) => {
                self.sign_in_state = SignInState::PickMode;
                self.error = Some(e.to_string());
                self.event_tx.send(AppEvent::RequestRedraw);
            }
        }
    }

    /// TODO: Read/write from the correct hierarchy config overrides + auth json + OPENAI_API_KEY.
    fn verify_api_key(&mut self) {
        if matches!(self.login_status, LoginStatus::AuthMode(AuthMode::ApiKey)) {
            // We already have an API key configured (e.g., from auth.json or env),
            // so mark this step complete immediately.
            self.sign_in_state = SignInState::EnvVarFound;
        } else {
            self.sign_in_state = SignInState::EnvVarMissing;
        }

        self.event_tx.send(AppEvent::RequestRedraw);
    }

    pub(crate) fn apply_chatgpt_login_side_effects(&mut self) {
        self.login_status = LoginStatus::AuthMode(AuthMode::ChatGPT);
        if let Ok(mut args) = self.chat_widget_args.lock() {
            args.config.using_chatgpt_auth = true;
            if args
                .config
                .model
                .eq_ignore_ascii_case("gpt-5")
            {
                let new_model = GPT_5_CODEX_MEDIUM_MODEL.to_string();
                args.config.model = new_model.clone();

                let family = find_family_for_model(&new_model)
                    .unwrap_or_else(|| derive_default_model_family(&new_model));
                args.config.model_family = family;
            }
        }
    }
}

async fn spawn_completion_poller(
    child: codex_login::LoginServer,
    event_tx: AppEventSender,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Ok(()) = child.block_until_done().await {
            event_tx.send(AppEvent::OnboardingAuthComplete(Ok(())));
        } else {
            event_tx.send(AppEvent::OnboardingAuthComplete(Err(
                "login failed".to_string()
            )));
        }
    })
}

impl StepStateProvider for AuthModeWidget {
    fn get_step_state(&self) -> StepState {
        match &self.sign_in_state {
            SignInState::PickProvider
            | SignInState::PickMode(_)
            | SignInState::EnvVarMissing
            | SignInState::ChatGptContinueInBrowser(_)
            | SignInState::ChatGptSuccessMessage
            | SignInState::ClaudeContinueInBrowser(_)
            | SignInState::ClaudeSuccessMessage => StepState::InProgress,
            SignInState::ChatGptSuccess 
            | SignInState::ClaudeSuccess 
            | SignInState::EnvVarFound => StepState::Complete,
        }
    }
}

impl WidgetRef for AuthModeWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        match &self.sign_in_state {
            SignInState::PickProvider => {
                self.render_pick_provider(area, buf);
            }
            SignInState::PickMode(_) => {
                self.render_pick_mode(area, buf);
            }
            SignInState::ChatGptContinueInBrowser(_) => {
                self.render_continue_in_browser(area, buf);
            }
            SignInState::ChatGptSuccessMessage => {
                self.render_chatgpt_success_message(area, buf);
            }
            SignInState::ChatGptSuccess => {
                self.render_chatgpt_success(area, buf);
            }
            SignInState::ClaudeContinueInBrowser(claude_state) => {
                self.render_claude_continue_in_browser(area, buf, claude_state);
            }
            SignInState::ClaudeSuccessMessage => {
                self.render_claude_success_message(area, buf);
            }
            SignInState::ClaudeSuccess => {
                self.render_claude_success(area, buf);
            }
            SignInState::EnvVarMissing => {
                self.render_env_var_missing(area, buf);
            }
            SignInState::EnvVarFound => {
                self.render_env_var_found(area, buf);
            }
        }
    }
}
