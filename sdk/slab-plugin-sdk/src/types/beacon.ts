/**
 * @slab/plugin-sdk — `slab.beacon.*` types.
 *
 * Beacon is Slab's AI layer (chat-with-PDF, summary, semantic search,
 * etc., shipping in v0.10.0). Plugins extend Beacon in two ways:
 *
 * 1. **Tool registration** — expose a callable function that the LLM
 *    can invoke during a chat session (e.g. "translate the current
 *    selection to French"). The LLM picks tools by id + schema.
 * 2. **AI provider registration** — register an alternative inference
 *    endpoint (a custom Ollama, an OpenAI-compatible proxy, etc.) so
 *    Beacon can route requests through it instead of the default.
 *
 * Surface ground truth: `src-tauri/src/plugins/runtime/slab_global.rs`
 * lines 117-127. The registrations accumulate into `Registrations`
 * (`runtime/host_api.rs`); Beacon consumes them post-enable.
 */

/**
 * Descriptor passed to `slab.beacon.registerTool(...)`.
 *
 * The descriptor is serialised as JSON and handed to Beacon. At call
 * time Beacon validates the LLM's argument blob against `parameters`
 * (JSON-Schema) before invoking `run`.
 */
export interface BeaconTool {
  /** Stable identifier — lowercase kebab-case recommended. */
  id: string;
  /** Display name shown in Beacon's tool picker. */
  name?: string;
  /** Free-form description; the LLM sees this in the tool roster. */
  description?: string;
  /**
   * JSON-Schema describing the parameters object. Use a minimal
   * schema — verbose schemas eat the model's attention budget.
   */
  parameters?: Record<string, unknown>;
  /**
   * Callback invoked when Beacon decides to call this tool. Return
   * value is JSON-serialised and handed back to the LLM. Throwing
   * surfaces a tool error to the model so it can retry / adapt.
   */
  run(input: unknown): Promise<unknown> | unknown;
}

/**
 * Descriptor passed to `slab.beacon.registerAiProvider(...)`.
 *
 * Registering an AI provider lets the user pick this plugin's
 * endpoint from the Beacon model-picker UI (lands with v0.10.0). The
 * provider is keyed by `id`; the Beacon settings panel surfaces
 * `label`. `kind` selects the wire protocol Beacon uses to talk to
 * `base_url`.
 */
export interface BeaconAiProvider {
  id: string;
  label: string;
  /**
   * Wire protocol. Beacon currently knows:
   *   - `openai-compatible` — POST /v1/chat/completions
   *   - `ollama` — POST /api/chat
   *   - `anthropic` — POST /v1/messages
   *   - `custom` — escape hatch; Beacon falls back to a thin pass-through
   */
  kind: "openai-compatible" | "ollama" | "anthropic" | "custom";
  /** Endpoint base URL — no trailing slash. */
  base_url: string;
  /** Model identifier sent on every request. */
  default_model: string;
  /** Extra headers (auth tokens, etc.). Keys + values must be strings. */
  headers?: Record<string, string>;
}

/**
 * The shape of `slab.beacon`. Bound when the plugin's declared
 * `BeaconCap` permits the matching registration; calling a method
 * the plugin doesn't have capability for throws a JS Error with a
 * message matching the Cabinet consent UI.
 */
export interface BeaconSurface {
  /**
   * Register a tool callable by Beacon's LLM. Requires
   * `BeaconCap === "tool-provider"` or `"both"`.
   */
  registerTool(tool: BeaconTool): void;
  /**
   * Register an alternative AI provider endpoint. Requires
   * `BeaconCap === "ai-provider"` or `"both"`.
   */
  registerAiProvider(provider: BeaconAiProvider): void;
}
