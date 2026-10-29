export function targetSyncProgressLines(progress, locale, tr) {
  if (!progress) {
    return [];
  }
  const connected = progress.connectionStatus === "connected";
  const connectionText = connected
    ? tr(locale, "已连接", "connected")
    : progress.connectionStatus === "error"
      ? tr(locale, "连接错误", "connection error")
      : tr(locale, "正在连接", "connecting");
  const handshakeText = progress.serverReady
    ? tr(locale, "已完成", "server ready")
    : tr(locale, "等待服务器 hello", "waiting for server hello");
  const snapshotText = progress.snapshotReceived
    ? tr(
      locale,
      `已收到：行动体 ${progress.totalAgentCount}，地点 ${progress.totalLocationCount}`,
      `received: ${progress.totalAgentCount} agents, ${progress.totalLocationCount} locations`,
    )
    : progress.snapshotRequested
      ? tr(
        locale,
        progress.snapshotRetryCount > 0
          ? `已请求首个世界快照，重试 ${progress.snapshotRetryCount} 次`
          : "已请求首个世界快照",
        progress.snapshotRetryCount > 0
          ? `first world snapshot requested, ${progress.snapshotRetryCount} retries`
          : "first world snapshot requested",
      )
      : tr(locale, "等待首个世界快照", "waiting for first world snapshot");
  const sessionText = progress.authSyncInFlight
    ? tr(locale, "正在同步玩家会话", "syncing player session")
    : tr(
      locale,
      `状态 ${progress.authRuntimeStatus || progress.authRegistrationStatus || "pending"}`,
      `status ${progress.authRuntimeStatus || progress.authRegistrationStatus || "pending"}`,
    );
  const visibilityText = tr(
    locale,
    `快照行动体 ${progress.totalAgentCount}，当前可控 ${progress.visibleAgentCount}`,
    `snapshot agents ${progress.totalAgentCount}, visible ${progress.visibleAgentCount}`,
  );
  const lines = [
    tr(locale, `连接：${connectionText}`, `Connection: ${connectionText}`),
    tr(locale, `握手：${handshakeText}`, `Handshake: ${handshakeText}`),
    tr(locale, `快照：${snapshotText}`, `Snapshot: ${snapshotText}`),
    tr(locale, `玩家会话：${sessionText}`, `Player session: ${sessionText}`),
    tr(locale, `可见性：${visibilityText}`, `Visibility: ${visibilityText}`),
  ];
  if (progress.lastError) {
    lines.push(tr(locale, `错误：${progress.lastError}`, `Error: ${progress.lastError}`));
  }
  return lines;
}
