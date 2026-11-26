export function resourceSummary(resources) {
  if (!resources || typeof resources !== "object") {
    return "-";
  }
  return Object.entries(resources)
    .map(([key, value]) => {
      if (value && typeof value === "object") {
        return `${key}:${JSON.stringify(value)}`;
      }
      return `${key}:${value}`;
    })
    .join(" · ") || "-";
}

export function buildViewerEntityLists({ entityCollections, selectedSearch, isAgentVisibleToCurrentSession }) {
  const { agents, locations } = entityCollections();
  const keyword = String(selectedSearch || "").trim().toLowerCase();
  const filter = (entry, label) => !keyword || String(label).toLowerCase().includes(keyword);
  return {
    agents: agents
      .filter((agent) => isAgentVisibleToCurrentSession(agent.id))
      .filter((agent) => filter(agent, `${agent.id} ${agent.location_id}`))
      .sort((a, b) => String(a.id).localeCompare(String(b.id))),
    locations: locations
      .filter((location) => filter(location, `${location.id} ${location.name}`))
      .sort((a, b) => String(a.id).localeCompare(String(b.id))),
  };
}
