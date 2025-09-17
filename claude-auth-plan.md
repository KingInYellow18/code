<generate Claude Code Provider to just-every/code: A Detailed Implementation PlanYes, it's absolutely possible to fork just-every/code and add Claude Code as a provider! Based on my analysis of how Cline successfully integrates Claude Code and the architecture of just-every/code, I can provide you with a comprehensive roadmap.

## Understanding the Current Architecturejust-every/code is built in Rust and currently supports OpenAI through two authentication methods:[1]
- **ChatGPT Sign-in**: For Plus/Pro/Team subscribers
- **API Key**: For usage-based billing

The tool orchestrates other AI CLI tools (mentioning Claude and Gemini support) but currently does this by **running separate processes** rather than direct integration.[1]

## How Cline Achieves Claude Code IntegrationCline's approach provides the perfect blueprint for your implementation. Here's how they do it:

### 1. **CLI Wrapper Architecture**Cline wraps the Claude Code CLI using Node.js's `execa` process manager. The integration:[2]
- Spawns a new `claude` process for each request
- Passes system prompts and messages as JSON input
- Streams JSON responses back through stdout parsing[2]

### 2. **Authentication Detection**Cline automatically detects whether you're using subscription or API authentication by examining the `apiKeySource` field in Claude Code's response. When it's "none", you're using subscription billing.[3]

### 3. **Message Preprocessing**Since Claude Code doesn't support images through CLI, Cline filters messages and converts image blocks to text placeholders:[2]

```rust
// Equivalent to Cline's message```ltering
if```ock.type == "image" {```  return Text```ck {
        type```text",
        text```ormat!("[Image ({}): {} not supported by Claude Code]",```               ```  source_type```edia_type)```  }
}
```## Implementation Strategy for just-every/code### Phase 1: Provider Interface DesignCreate a provider abstraction that can handle both OpenAI and Claude Code:

```rust
trait AIProvider {
    fn authenticate```elf) -> Result<Auth```tus>;
    fn create```ssage(&self, system```tring, messages```ec<Message```-> Result<Response```eam>;
    fn```t_model(&self) -> Model```o;
    fn supports```ages(&self) ->```ol;
}
```

### Phase 2: Claude Code Provider Implementation**File Structure:**
```
src/
├── providers/
│   ├── mod.rs
│```├── openai.rs          ```xisting OpenAI provider```  ├── claude_```e.rs     # New```aude Code provider```  └── factory```         # Provider```ctory
├── config```   └── mod.rs```         # Extende```onfig for Claude```de path
└── types```   └── messages```       # Message filtering```gic
```

**Key Implementation Components:**

1. **Process Management**
```rust
use tokio::process::Command;
use tok```:io::{BufReader, Async```ReadExt};

pub struct```audeCodeProvider```    claude```th: String```   model_i```String,
}

impl Clau```odeProvider {```  async fn spawn_claude```ocess(&self, system```ompt: &str, messages```[Message]) -> Result```ild> {
        ``` mut cmd = Command::```(&self.claude_path);
        cmd.args```[
            "--system-prompt", system_prompt,
            "--verbose",
            "--output-format", "stream-json",
            "--max-turns", "1",
            "--model", &self.model_id,
            "-p"
        ]);```      
        let mut chil``` cmd
            .stdin```dio::piped())
            .```out(Stdio::piped())
            .stderr```dio::piped())
            .spawn(```
            ```      // Sen```essages as```ON to stdin```      let stdin```child.stdin```_mut().unwrap();
        ```in.write_all(serde_json::```string(&messages)?.as_bytes```.await?;
        stdin.```sh().await?;
        drop```ild.stdin.take());
        
        Ok```ild)
    }
}````

2. **Message Filtering**
```rust
fn filter_messages_for_claude_```e(messages: Vec```ssage>) ->```c<Message>```    messages```to_iter().map(|mut msg```
        if let```ssageContent```rray(ref mut```ocks) = msg.```tent {
            ``` block in blocks.```r_mut() {
                if```t ContentBlock```mage(img_block) = block```                    *```ck = Content```ck::Text(TextBlock {
                ```     text: format!("[Image: {} not supported by Claude Code]",```                                   ```_block.source.media_type),```                  ```
                }```          }```      }
        ```
    }).collect()
}````

3. **Response Stream Parsing**
```rust
async fn parse_claude_response```elf, stdout```hildStdout)``` ResponseStream {
    ``` reader = BufReader```ew(stdout);
    let mut```nes = reader.lines```
    
    while```t Some(line``` lines.next_```e().await? {
        if```t Ok(chunk``` serde_json::```m_str::<ClaudeCodeMessage```line) {
            match```unk.message```pe.as_str() {
                "```istant" => yiel```esponseChunk::Text```unk.content),
                "```tem" => {
                    //```tect subscription``` API usage
                    self```_subscription = chunk```i_key_source == "none```                }```              "result```> {
                    yiel```esponseChunk::```ge(UsageStats {
                        ```t: if self.is_```scription { 0```} else { chunk```tal_cost_usd }```                  });```              }```          ```       }
    }``````

### Phase 3: Configuration IntegrationExtend just-every/code's existing configuration system:

```toml
# ~/.codex/config.toml
[providers.claude_code]```abled = true
claude```th = "claude```# or full path like "/usr```cal/bin/claude"
default_model =```laude-sonnet-4-```50514"
timeout_ms = 600```

[profiles.claude-max]
provider```"claude_code"
model```"claude-sonnet-4```250514"
approval_policy```"on_request`````

### Phase 4: Multi-Agent Command IntegrationEnhance the existing `/plan`, `/solve`, `/code` commands to include Claude Code:

```rust
async fn execute_multi_agent_comman```self, command:```tr, prompt```str) -> Result<()```
    let providers```vec![
        self.get_provider("openai")?,
        self.get_provider("claude_code")?,  // Your new provider
        self.get_provider("gemini")?,       // If implemented
    ];```  
    match```mmand {
        ```lan" => self```nsensus_mode(providers, prompt).```it,
        "/solve```> self.race_mode(providers```rompt).await```       "/code" => self```nsensus_mode(providers```rompt).await```   }
}
```

## Technical Challenges and Solutions### Challenge 1: Authentication Complexity**Solution**: Abstract authentication behind the provider interface. Claude Code handles its own auth through `claude auth status`, so your provider just needs to verify the CLI is authenticated.

### Challenge 2: Response Format Differences**Solution**: Create response normalizers that convert each provider's output format into a common internal format.

### Challenge 3: Image Support Differences**Solution**: Implement capability detection per provider and gracefully handle unsupported features with clear user feedback.

### Challenge 4: Error Handling Variations**Solution**: Create provider-specific error mappers that translate each system's errors into standardized error types.

## Development Workflow1. **Start Small**: Fork just-every/code and add basic Claude Code support for single requests
2. **Test Authentication**: Ensure both subscription and API key scenarios work correctly  
3. **Add Message Filtering**: Implement image-to-text conversion for Claude Code compatibility
4. **Integrate Multi-Agent**: Enhance the existing multi-agent commands to include Claude Code
5. **Polish UX**: Add provider selection, model switching, and error handling

## Why This Approach WorksThis strategy leverages proven patterns:

- **Cline's Success**: Their Claude Code integration serves millions of developers[3]
- **Provider Pattern**: Industry-standard approach for multi-provider systems[4]
- **CLI Wrapping**: Mature technique used by many developer tools[5]
- **Process Isolation**: Safer than direct API integration for experimental features

The beauty of this approach is that you get to **use your Claude Max subscription** without paying API rates while maintaining just-every/code's existing functionality and adding powerful multi-agent capabilities.

Your implementation would essentially create a "best of both worlds" solution: just-every/code's enhanced features with Claude Code's subscription-friendly economics and powerful reasoning capabilities.

