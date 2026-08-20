// Re-export scenario-owned prompts so the shared harness has one stable import.
export { taskPrompt as multiAgentTaskPrompt } from "./multi-agent.mjs";
export { taskPrompt as singleAgentTaskPrompt } from "./single-agent.mjs";
export { taskPrompt as toolCapabilityTaskPrompt } from "./tool-capability.mjs";
