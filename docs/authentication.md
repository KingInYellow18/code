# Authentication

## Multi-Provider Authentication

Code supports multiple authentication providers to give you the best AI experience:

- **Claude Max/Pro**: Premium subscription with unlimited messages and priority access
- **OpenAI ChatGPT**: Plus/Pro/Team plans with usage-based on your subscription
- **API Keys**: Usage-based billing for both Claude and OpenAI

### Quick Setup

```bash
# Authenticate with Claude Max (recommended)
code auth login --provider claude

# Authenticate with OpenAI ChatGPT
code auth login --provider openai

# Use auto-selection (tries Claude first, falls back to OpenAI)
code auth login

# Check status
code auth status --detailed
```

## Claude Authentication

### Claude Max/Pro Subscription (Recommended)

Claude Max provides unlimited conversations and enhanced performance:

```bash
# Authenticate with Claude subscription
code auth login --provider claude

# Check subscription status and quota
code auth status --provider claude --detailed
code auth quota --detailed
```

### Claude API Key

For usage-based billing with Claude:

```shell
export ANTHROPIC_API_KEY="sk-ant-api03-your-key-here"
```

Or authenticate directly:
```bash
code auth login --provider claude --api-key sk-ant-api03-...
```

## OpenAI Authentication

### Usage-based billing alternative: Use an OpenAI API key

If you prefer to pay-as-you-go, you can still authenticate with your OpenAI API key by setting it as an environment variable:

```shell
export OPENAI_API_KEY="your-api-key-here"
```

This key must, at minimum, have write access to the Responses API.

## Migrating to ChatGPT login from API key

If you've used the Codex CLI before with usage-based billing via an API key and want to switch to using your ChatGPT plan, follow these steps:

1. Update the CLI and ensure `codex --version` is `0.20.0` or later
2. Delete `~/.codex/auth.json` (on Windows: `C:\\Users\\USERNAME\\.codex\\auth.json`)
3. Run `codex login` again

## Provider Selection and Preferences

### Automatic Provider Selection

Code intelligently selects the best available provider:

```bash
# Enable auto-selection
code auth switch auto

# Check which provider is being used
code auth status --detailed
```

Selection priority:
1. Claude Max subscription (if authenticated)
2. Claude Pro subscription (if authenticated) 
3. OpenAI ChatGPT (if authenticated)
4. Claude API key (if available)
5. OpenAI API key (if available)

### Manual Provider Selection

You can explicitly choose which provider to use:

```bash
# Switch to Claude
code auth switch claude

# Switch to OpenAI
code auth switch openai

# Check current provider
code auth status
```

### Configuration File Settings

```toml
# ~/.codex/config.toml
preferred_auth_provider = "claude"  # "claude" | "openai" | "auto"

[claude]
auto_fallback_enabled = true       # Fall back to OpenAI if Claude quota exhausted
quota_warning_threshold = 0.8      # Warn at 80% quota usage

# Provider-specific profiles
[profiles.claude-max]
model = "claude-3-opus-20240229"
model_provider = "claude"
approval_policy = "never"

[profiles.openai-gpt4]
model = "gpt-4"
model_provider = "openai"
approval_policy = "on_request"
```

### CLI Override

```bash
# Use specific provider for one command
code --provider claude "Analyze this code"
code --provider openai "Generate documentation"

# Override configuration
code --config preferred_auth_provider="claude" "Hello"
```

## Legacy OpenAI Configuration

### Forcing a specific auth method (advanced)

You can explicitly choose which OpenAI authentication method to prefer:

- To always use your API key (even when ChatGPT auth exists), set:

```toml
# ~/.codex/config.toml
preferred_auth_method = "apikey"
```

- To prefer ChatGPT auth (default), set:

```toml
# ~/.codex/config.toml
preferred_auth_method = "chatgpt"
```

Notes:

- When `preferred_auth_method = "apikey"` and an API key is available, the login screen is skipped.
- When `preferred_auth_method = "chatgpt"` (default), Code prefers ChatGPT auth if present; if only an API key is present, it will use the API key.
- To check which auth method is being used during a session, use the `/status` command in the TUI or `code auth status --detailed`.

## Project .env safety

### API Key Environment Variables

By default, Code will no longer read `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or `CLAUDE_API_KEY` from a project's local `.env` file.

**Why:** Many repos include API keys in `.env` for unrelated tooling, which could cause Code to silently use the API key instead of your preferred subscription plan.

**What still works:**

- `~/.code/.env` (or `~/.codex/.env`) is loaded first and may contain API keys for global use
- Shell-exported environment variables are honored:
  ```bash
  export OPENAI_API_KEY="sk-..."
  export ANTHROPIC_API_KEY="sk-ant-api03-..."
  export CLAUDE_API_KEY="sk-ant-api03-..."  # Alternative name, auto-mapped
  ```

**Project `.env` provider keys are always ignored** — there is no opt‑in.

### UI clarity

The TUI shows clear provider information:

- **Claude Max**: "Auth: Claude Max" with quota indicator
- **Claude API**: "Auth: Claude API" badge
- **OpenAI ChatGPT**: "Auth: ChatGPT" badge  
- **OpenAI API**: "Auth: OpenAI API" badge

Check current provider anytime:
```bash
code auth status --detailed
```

## Connecting on a "Headless" Machine

Today, the login process entails running a server on `localhost:1455`. If you are on a "headless" server, such as a Docker container or are `ssh`'d into a remote machine, loading `localhost:1455` in the browser on your local machine will not automatically connect to the webserver running on the _headless_ machine, so you must use one of the following workarounds:

### Authenticate locally and copy your credentials to the "headless" machine

The easiest solution is likely to run through the `codex login` process on your local machine such that `localhost:1455` _is_ accessible in your web browser. When you complete the authentication process, an `auth.json` file should be available at `$CODEX_HOME/auth.json` (on Mac/Linux, `$CODEX_HOME` defaults to `~/.codex` whereas on Windows, it defaults to `%USERPROFILE%\\.codex`).

Because the `auth.json` file is not tied to a specific host, once you complete the authentication flow locally, you can copy the `$CODEX_HOME/auth.json` file to the headless machine and then `codex` should "just work" on that machine. Note to copy a file to a Docker container, you can do:

```shell
# substitute MY_CONTAINER with the name or id of your Docker container:
CONTAINER_HOME=$(docker exec MY_CONTAINER printenv HOME)
docker exec MY_CONTAINER mkdir -p "$CONTAINER_HOME/.codex"
docker cp auth.json MY_CONTAINER:"$CONTAINER_HOME/.codex/auth.json"
```

whereas if you are `ssh`'d into a remote machine, you likely want to use [`scp`](https://en.wikipedia.org/wiki/Secure_copy_protocol):

```shell
ssh user@remote 'mkdir -p ~/.codex'
scp ~/.codex/auth.json user@remote:~/.codex/auth.json
```

or try this one-liner:

```shell
ssh user@remote 'mkdir -p ~/.codex && cat > ~/.codex/auth.json' < ~/.codex/auth.json
```

### Connecting through VPS or remote

If you run Codex on a remote machine (VPS/server) without a local browser, the login helper starts a server on `localhost:1455` on the remote host. To complete login in your local browser, forward that port to your machine before starting the login flow:

```bash
# From your local machine
ssh -L 1455:localhost:1455 <user>@<remote-host>
```

Then, in that SSH session, run `codex` and select "Sign in with ChatGPT". When prompted, open the printed URL (it will be `http://localhost:1455/...`) in your local browser. The traffic will be tunneled to the remote server. 
