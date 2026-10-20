export function Badge(props) {
  return <span class={props.class ?? "badge"}>{props.children}</span>;
}
export function EmptyState(props) {
  return <div class={`empty ${props.class ?? ""}`} style={props.style}>{props.children}</div>;
}
