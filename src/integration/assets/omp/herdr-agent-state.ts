// installed by herdr
// managed by herdr; reinstalling or updating the integration overwrites this file.
// add custom hooks/plugins beside this file instead of editing it.
// HIVE_INTEGRATION_ID=omp
// HIVE_INTEGRATION_VERSION=9
// @ts-nocheck

import net from "node:net";

const HIVE_ENV = process.env.HIVE_ENV;
const socketPath = process.env.HIVE_SOCKET_PATH;
const socketEndpoint =
  process.platform === "win32" && socketPath ? `\\\\.\\pipe\\${socketPath}` : socketPath;
const paneId = process.env.HIVE_PANE_ID;
const source = "hive:omp";
const agentDebugEnabled = process.env.HIVE_AGENT_DEBUG === "1";

function debugLog(event: string, fields: Record<string, unknown> = {}): void {
  if (!agentDebugEnabled) {
    return;
  }
  console.debug(`[HiveAgent] ${source} ${event}`, JSON.stringify(fields));
}

function enabled() {
  return HIVE_ENV === "1" && !!socketPath && !!paneId;
}

let requestQueue = Promise.resolve();

function sendRequestAttempt(request: unknown, timeoutMs: number): Promise<boolean> {
  if (!enabled()) {
    return Promise.resolve(true);
  }

  const method = request && typeof request === "object" ? request.method : undefined;
  debugLog("socket_attempt", {
    method,
    endpoint: socketEndpoint,
    timeoutMs,
  });

  const { promise, resolve } = Promise.withResolvers<boolean>();
  let done = false;
  let timeout: ReturnType<typeof setTimeout> | undefined;
  const finish = (delivered: boolean) => {
    if (done) return;
    done = true;
    clearTimeout(timeout);
    debugLog("socket_result", {
      method,
      endpoint: socketEndpoint,
      timeoutMs,
      delivered,
    });
    socket.destroy();
    resolve(delivered);
  };

  const socket = net.createConnection(socketEndpoint!);
  socket.on("error", () => finish(false));
  socket.on("connect", () => socket.write(`${JSON.stringify(request)}\n`));
  socket.on("data", () => finish(true));
  socket.on("end", () => finish(false));
  timeout = setTimeout(() => finish(false), timeoutMs);
  timeout.unref?.();
  return promise;
}

async function sendRequestNow(request: unknown): Promise<void> {
  if (await sendRequestAttempt(request, 500)) {
    return;
  }
  await sendRequestAttempt(request, 1500);
}

function sendRequest(request: unknown): Promise<void> {
  requestQueue = requestQueue.then(
    () => sendRequestNow(request),
    () => sendRequestNow(request),
  );
  return requestQueue;
}

type AgentState = "working" | "blocked" | "idle";

type QueuedState = {
  state: AgentState;
  message?: string;
  seq: number;
};

const idleDebounceMs = parseDurationEnv("HIVE_OMP_IDLE_DEBOUNCE_MS", 250);
const retryGraceMs = parseDurationEnv("HIVE_OMP_RETRY_GRACE_MS", 2500);
const retryableErrorPattern =
  /overloaded|provider.?returned.?error|rate.?limit|too many requests|429|500|502|503|504|service.?unavailable|server.?error|internal.?error|network.?error|connection.?error|connection.?refused|connection.?lost|websocket.?closed|websocket.?error|other side closed|fetch failed|upstream.?connect|reset before headers|socket hang up|ended without|http2 request did not get a response|timed? out|timeout|terminated|retry delay/i;
let reportSeq = Date.now() * 1000;
let currentAgentSessionId: string | undefined;
let currentAgentSessionPath: string | undefined;

function nextReportSeq(): number {
  reportSeq += 1;
  return reportSeq;
}

function isAbsoluteSessionPath(value: unknown): value is string {
  return (
    typeof value === "string" &&
    (value.startsWith("/") || /^[A-Za-z]:[\\/]/.test(value) || value.startsWith("\\\\"))
  );
}

function updateSessionRef(ctx: any): void {
  try {
    const file = ctx?.sessionManager?.getSessionFile?.();
    currentAgentSessionPath = isAbsoluteSessionPath(file) ? file : undefined;
  } catch {
    currentAgentSessionPath = undefined;
  }

  try {
    const id = ctx?.sessionManager?.getSessionId?.();
    currentAgentSessionId = typeof id === "string" && id.length > 0 ? id : undefined;
  } catch {
    currentAgentSessionId = undefined;
  }
}

function withSessionRef(params: Record<string, unknown>): Record<string, unknown> {
  if (currentAgentSessionPath) {
    return { ...params, agent_session_path: currentAgentSessionPath };
  }
  if (currentAgentSessionId) {
    return { ...params, agent_session_id: currentAgentSessionId };
  }
  return params;
}

function parseDurationEnv(name: string, fallback: number): number {
  const raw = process.env[name];
  if (!raw) {
    return fallback;
  }
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return fallback;
  }
  return parsed;
}

function currentSessionRef(): Record<string, unknown> | undefined {
  if (currentAgentSessionPath) {
    return { agent_session_path: currentAgentSessionPath };
  }
  if (currentAgentSessionId) {
    return { agent_session_id: currentAgentSessionId };
  }
  return undefined;
}

function reportSession(sessionStartSource = "startup"): Promise<void> {
  const sessionRef = currentSessionRef();
  if (!sessionRef) {
    debugLog("session_report_skipped", {
      reason: "missing_session_ref",
      session_start_source: sessionStartSource,
      session_path: currentAgentSessionPath,
      session_id: currentAgentSessionId,
    });
    return Promise.resolve();
  }

  const seq = nextReportSeq();
  debugLog("session_report", {
    session_start_source: sessionStartSource,
    seq,
    session_path: currentAgentSessionPath,
    session_id: currentAgentSessionId,
  });
  return sendRequest({
    id: `${source}:session:${Date.now()}:${Math.random().toString(36).slice(2)}`,
    method: "pane.report_agent_session",
    params: {
      pane_id: paneId,
      source,
      agent: "omp",
      seq,
      session_start_source: sessionStartSource,
      ...sessionRef,
    },
  });
}

function sendState(state: AgentState, message?: string, seq = nextReportSeq()): Promise<void> {
  const params = withSessionRef({
    pane_id: paneId,
    source,
    agent: "omp",
    state,
    message,
    seq,
  });
  debugLog("state_report", {
    state,
    seq,
    session_path: currentAgentSessionPath,
    session_id: currentAgentSessionId,
  });
  return sendRequest({
    id: `${source}:${Date.now()}:${Math.random().toString(36).slice(2)}`,
    method: "pane.report_agent",
    params,
  });
}

let sendInFlight = false;
let queuedState: QueuedState | undefined;

function queueState(state: AgentState, message?: string): void {
  const previous = queuedState;
  const seq = nextReportSeq();
  queuedState = { state, message, seq };
  if (previous) {
    debugLog("state_queue_replace", {
      previous_state: previous.state,
      previous_seq: previous.seq,
      state,
      seq,
    });
  }
  if (!sendInFlight) {
    void drainStateQueue();
  }
}

async function drainStateQueue(): Promise<void> {
  if (sendInFlight) {
    return;
  }

  sendInFlight = true;
  try {
    while (queuedState) {
      const next = queuedState;
      queuedState = undefined;
      await sendState(next.state, next.message, next.seq);
    }
  } finally {
    sendInFlight = false;
    if (queuedState) {
      void drainStateQueue();
    }
  }
}

function lastAssistantMessage(messages: unknown[]): any | undefined {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const message = messages[i] as any;
    if (message?.role === "assistant") {
      return message;
    }
  }
  return undefined;
}

function retryableErrorMessage(event: any): string | undefined {
  const messages = Array.isArray(event?.messages) ? event.messages : [];
  const assistant = lastAssistantMessage(messages);
  if (assistant?.stopReason !== "error") {
    return undefined;
  }

  const errorMessage = String(assistant.errorMessage ?? "");
  if (!retryableErrorPattern.test(errorMessage)) {
    return undefined;
  }
  return errorMessage || "retryable provider error";
}

function askBlockedMessage(args: any): string {
  const questions = Array.isArray(args?.questions) ? args.questions : [];
  const firstQuestion = questions.find((question: any) => typeof question?.question === "string");
  if (firstQuestion?.question) {
    return firstQuestion.question;
  }
  return "waiting for user input";
}

export default function (pi) {
  if (!enabled()) {
    return;
  }

  let agentActive = false;
  let retryHoldActive = false;
  let failureBlocked = false;
  let failureMessage: string | undefined;
  let blockedCount = 0;
  let blockedMessage: string | undefined;
  let lastState: AgentState | undefined;
  let lastMessage: string | undefined;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let rootSession = false;

  function clearTimer(timer: ReturnType<typeof setTimeout> | undefined) {
    if (timer) {
      clearTimeout(timer);
    }
  }

  function clearPendingTimers() {
    clearTimer(idleTimer);
    clearTimer(retryTimer);
    idleTimer = undefined;
    retryTimer = undefined;
  }

  function clearFailureState() {
    retryHoldActive = false;
    failureBlocked = false;
    failureMessage = undefined;
  }

  function desiredState() {
    if (blockedCount > 0) {
      return { state: "blocked" as const, message: blockedMessage };
    }
    if (failureBlocked) {
      return { state: "blocked" as const, message: failureMessage };
    }
    if (agentActive || retryHoldActive) {
      return { state: "working" as const, message: undefined };
    }
    return { state: "idle" as const, message: undefined };
  }

  function publishState(force = false) {
    const next = desiredState();
    if (!force && next.state === lastState && next.message === lastMessage) {
      return;
    }
    lastState = next.state;
    lastMessage = next.message;
    queueState(next.state, next.message);
  }

  function scheduleIdle() {
    clearPendingTimers();
    clearFailureState();
    idleTimer = setTimeout(() => {
      idleTimer = undefined;
      publishState();
    }, idleDebounceMs);
    idleTimer.unref?.();

    publishState();
  }

  function scheduleRetryHold() {
    clearTimer(retryTimer);
    retryTimer = setTimeout(() => {
      retryTimer = undefined;
      if (retryHoldActive) {
        retryHoldActive = false;
        publishState();
      }
    }, retryGraceMs);
    retryTimer.unref?.();
  }

  pi.events.on("herdr:blocked", (data) => {
    if (!rootSession) {
      return;
    }
    if (!data?.active) {
      blockedCount = Math.max(0, blockedCount - 1);
      if (blockedCount === 0) {
        blockedMessage = undefined;
      }
      publishState();
      return;
    }

    blockedCount += 1;
    blockedMessage = data.label;
    publishState();
  });

  pi.on("session_start", async (event, ctx) => {
    if (ctx?.hasUI !== true) {
      return;
    }
    rootSession = true;
    updateSessionRef(ctx);
    const isIdle = ctx?.isIdle?.() === true;
    debugLog("lifecycle", {
      event: "session_start",
      reason: event?.reason,
      is_idle: isIdle,
      session_path: currentAgentSessionPath,
      session_id: currentAgentSessionId,
    });
    await reportSession(event?.reason);
    // A reload can replace this extension mid-run without emitting another agent_start.
    agentActive = !isIdle;
    publishState(true);
  });

  pi.on("agent_start", (_event, ctx) => {
    if (!rootSession) {
      return;
    }
    updateSessionRef(ctx);
    debugLog("lifecycle", {
      event: "agent_start",
      session_path: currentAgentSessionPath,
      session_id: currentAgentSessionId,
    });
    void reportSession();
    agentActive = true;
    clearFailureState();
    publishState();
  });

  function contextIsIdle(ctx: unknown): boolean {
    if (typeof ctx !== "object" || ctx === null || !("isIdle" in ctx)) {
      return false;
    }
    const isIdle = ctx.isIdle;
    return typeof isIdle === "function" && isIdle() === true;
  }

  function handleAgentCompletion(event: unknown, ctx: unknown): void {
    if (!rootSession || !agentActive) {
      return;
    }

    agentActive = false;
    debugLog("lifecycle", {
      event: typeof event === "string" ? event : "completion",
      is_idle: contextIsIdle(ctx),
      retry_hold_active: retryHoldActive,
    });

    const retryableMessage = retryableErrorMessage(event);
    if (retryableMessage) {
      clearPendingTimers();
      retryHoldActive = true;
      failureBlocked = false;
      failureMessage = retryableMessage;
      scheduleRetryHold();
      publishState();
      return;
    }

    scheduleIdle();
  }

  // OMP's current extension API emits agent_end after the completed output.
  // Keep agent_settled for older runtimes that exposed the Pi-compatible name.
  pi.on("agent_end", (event, ctx) => {
    handleAgentCompletion(event, ctx);
  });

  pi.on("agent_settled", (_event, ctx) => {
    const isIdle = ctx?.isIdle?.() === true;
    if (!rootSession || !isIdle) {
      if (rootSession) {
        debugLog("lifecycle", { event: "agent_settled_ignored", is_idle: isIdle });
      }
      return;
    }
    handleAgentCompletion("agent_settled", ctx);
  });

  pi.on("agent_error", (event, ctx) => {
    if (!rootSession) {
      return;
    }

    const errorMessage = retryableErrorMessage(event);
    if (!errorMessage) {
      return;
    }
    debugLog("lifecycle", {
      event: "agent_error",
      retryable: true,
      message: errorMessage,
    });

    // Enter a retry hold to keep the pane in working state for a grace
    // period. The hold is released either when the retry grace timer
    // expires or when the next agent_start arrives (whichever comes
    // first). If the error persists after the grace window, the pane
    // will transition to blocked.
    retryHoldActive = true;
    failureBlocked = true;
    failureMessage = errorMessage;
    clearPendingTimers();
    scheduleRetryHold();
    publishState();
  });

  pi.on("session_end", () => {
    debugLog("lifecycle", { event: "session_end" });
    rootSession = false;
    clearPendingTimers();
    clearFailureState();
    agentActive = false;
    blockedCount = 0;
    blockedMessage = undefined;
    lastState = undefined;
    lastMessage = undefined;
  });
}
