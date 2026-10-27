import { describe, expect, it } from "vitest";
import vectors from "../../oasis7_game_client/src/operation_lifecycle_vectors.json";
import { nextOperationAction, operationObservation, retainTerminalObservation } from "./browser_operation_lifecycle.js";
describe("operation lifecycle shared contract", () => {
  it.each(vectors)("$name", ({ input, expected }) => expect(nextOperationAction(input)).toBe(expected));
  it("never promotes an unverified response and preserves a trusted terminal", () => {
    const unknown = operationObservation({ status: "committed" });
    expect(unknown.disposition).toBe("unknown");
    const terminal = operationObservation({ status: "committed", verified: true });
    expect(retainTerminalObservation(terminal, operationObservation({ status: "pending", verified: true }))).toBe(terminal);
    expect(retainTerminalObservation(terminal, unknown)).toBe(terminal);
  });
});
