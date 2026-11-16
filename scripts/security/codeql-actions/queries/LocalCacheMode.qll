/** Conservative local cache-mode proof; unknown YAML keeps the upstream finding. */
import actions
import codeql.actions.ast.internal.Ast
import codeql.actions.ast.internal.Yaml

private predicate unambiguousKeys(YamlMapping mapping) {
  not exists(YamlNode key |
    key = mapping.getKeyNode(_) and
    // The extractor may tag the workflow's literal `on` key as a YAML boolean.
    (not key instanceof YamlScalar or key.(YamlScalar).getValue() = "<<")
  )
}

private predicate hasCacheMode(YamlMapping mapping) {
  mapping.getKeyNode(_).(YamlString).getValue() = "cache-mode"
}

private predicate literalReadOrNone(YamlMapping mapping) {
  unambiguousKeys(mapping) and
  count(int i | mapping.getKeyNode(i).(YamlString).getValue() = "cache-mode") = 1 and
  exists(int i |
    mapping.getKeyNode(i).(YamlString).getValue() = "cache-mode" and
    // Inspect the raw node: evaluating it would silently follow scalar aliases.
    mapping.getValueNode(i).(YamlString).getValue() = ["read", "none"]
  )
}

/** Job overrides workflow. A present but unknown job value never inherits a safe default. */
predicate provenLocalReadOrNone(LocalJob job) {
  exists(YamlMapping local |
    local = job.(JobImpl).getNode() and
    unambiguousKeys(local) and
    (
      literalReadOrNone(local)
      or
      not hasCacheMode(local) and
      literalReadOrNone(job.getWorkflow().(WorkflowImpl).getNode())
    )
  )
}
