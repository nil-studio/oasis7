import { render, screen } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";
import { SafeAlternativePanel } from "./safe_alternative_panel.jsx";
it("renders exact scope and result with distinct confirmation/execution controls", () => {
 const record={request_id:"r",state:"awaiting_confirmation",original:{goal:"route",waypoints:[{x_cm:1,y_cm:0,z_cm:0}],arrival_tick:10},candidates:[{
 candidate_id:"c",goal:"direct move",to:{x_cm:1,y_cm:0,z_cm:0},scope:{agent_id:"a",from:{x_cm:0,y_cm:0,z_cm:0},to:{x_cm:1,y_cm:0,z_cm:0},maximum_effects:1},cost_upper_bound:0,risk:"position_change",consequences:"move",retractability:"before execute",recommendation:"endpoint",required_authority:"current_agent_owner"}]};
 render(() => <SafeAlternativePanel core={{state:{safeAlternative:{record,status:record.state},logicalTime:1},safeAlternative:{command:vi.fn(),recover:vi.fn()}}} />);
 expect(screen.getByText(/one direct move from/)).toBeInTheDocument();
 expect(screen.getByText(/Position changes immediately/)).toBeInTheDocument();
 expect(screen.getByText(/Current Agent owner/)).toBeInTheDocument();
 expect(document.body.textContent).not.toMatch(/awaiting_confirmation|position_change|current_agent_owner|maximum_effects/);
 expect(screen.getByText(/Execute confirmed choice/)).toBeDisabled();
 expect(screen.getByText(/Confirm this exact candidate/)).toBeEnabled();
 expect(screen.getByText(/Requested arrival tick:/)).toHaveTextContent("10");
});
