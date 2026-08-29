import type {
  OperonRunRequest,
  OperonStep,
  OperonStepTrace,
  OperonTrace,
} from "./types";
import type { RequestContext } from "./transport";

export type GraphActionRunner = (
  step: OperonStep,
  context: RequestContext,
) => Promise<unknown>;

export async function runGraph(
  request: OperonRunRequest,
  runAction: GraphActionRunner,
): Promise<OperonTrace> {
  const trace: OperonTrace = {
    runId: `run-${Date.now()}`,
    name: request.name,
    status: "running",
    steps: [],
  };

  for (const [index, step] of request.steps.entries()) {
    const stepTrace = await runStep(step, index, trace.runId, runAction);
    trace.steps.push(stepTrace);
    if (stepTrace.status === "failed") {
      trace.status = "failed";
      return trace;
    }
  }

  trace.status = "succeeded";
  return trace;
}

async function runStep(
  step: OperonStep,
  index: number,
  runId: string,
  runAction: GraphActionRunner,
): Promise<OperonStepTrace> {
  const startedAtMs = Date.now();
  const id = step.id ?? `step-${index + 1}`;

  try {
    const output = await runAction(step, { runId, stepId: id });
    return {
      id,
      node: step.node,
      action: step.action,
      status: "succeeded",
      startedAtMs,
      endedAtMs: Date.now(),
      output,
    };
  } catch (error) {
    return {
      id,
      node: step.node,
      action: step.action,
      status: "failed",
      startedAtMs,
      endedAtMs: Date.now(),
      error: error instanceof Error ? error.message : String(error),
    };
  }
}
