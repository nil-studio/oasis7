import { CheckCircle2, Archive, Clock, GitPullRequest, GitMerge, GitPullRequestClosed, CircleHelp, MessageCircleQuestion, ExternalLink, Compass, BookOpen, Hammer, Wrench, Globe, Users, Monitor, Network, Files } from 'lucide-solid';
export function StatusIcon(props:{value:string}) {
  const icon=()=>({'PR OPEN':GitPullRequest,'PR MERGED':GitMerge,'PR CLOSED':GitPullRequestClosed,open:MessageCircleQuestion,awaitingEvidence:Clock,resolved:CheckCircle2,superseded:Archive,external:ExternalLink} as Record<string,typeof CircleHelp>)[props.value]??CircleHelp;
  return <span class="status-icon" data-status={props.value}>{icon()({size:18,'aria-hidden':true})}</span>;
}
export function NavigationIcon(props:{view:string}) {
  const icon=()=>({direction:Compass,rules:BookOpen,execution:Hammer,attention:MessageCircleQuestion,tools:Wrench} as Record<string,typeof Compass>)[props.view]??Files;
  return <span class="navigation-icon">{icon()({size:20,'aria-hidden':true})}</span>;
}
export function DomainIcon(props:{path:string|null}) {
  const icon=()=>props.path?.includes('world-rules-core-gameplay')?Globe:props.path?.includes('agents-world-simulation')?Users:props.path?.includes('player-entry-distribution')?Monitor:props.path?.includes('world-infrastructure')?Network:Files;
  return <span class="domain-icon">{icon()({size:28,'aria-hidden':true})}</span>;
}
