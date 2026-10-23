import { BookOpen, Wrench, Globe, Users, Monitor, Network, Files } from 'lucide-solid';
export function NavigationIcon(props:{view:string}) {
  const icon=()=>({documents:BookOpen,tools:Wrench} as Record<string,typeof Files>)[props.view]??Files;
  return <span class="navigation-icon">{icon()({size:20,'aria-hidden':true})}</span>;
}
export function DomainIcon(props:{path:string|null}) {
  const icon=()=>props.path?.includes('world-rules-core-gameplay')?Globe:props.path?.includes('agents-world-simulation')?Users:props.path?.includes('player-entry-distribution')?Monitor:props.path?.includes('world-infrastructure')?Network:Files;
  return <span class="domain-icon">{icon()({size:28,'aria-hidden':true})}</span>;
}
