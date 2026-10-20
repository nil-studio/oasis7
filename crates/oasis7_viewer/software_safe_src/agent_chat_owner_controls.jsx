import { CanonicalAgentGoal } from "./canonical_agent_goal.jsx";
import { SafeAlternativePanel } from "./safe_alternative_panel.jsx";

// Both controls use the authenticated owner read while retaining independent
// signed commands. Ordinary chat never implicitly confirms an alternative.
export function AgentChatOwnerControls(props) {
  return <>
    <SafeAlternativePanel core={props.core} />
    <CanonicalAgentGoal state={props.core.state} agentId={props.agentId} locale={props.locale} />
  </>;
}
