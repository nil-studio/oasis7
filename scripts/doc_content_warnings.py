"""Separate writing advice from factual/integrity failures.

The historical inventory deliberately uses ``report_findings(..., True)``.
Changed-document gates use an explicit code allowlist instead: an unknown code
or a broken declared relation must never become advisory through a CLI flag.
"""


def finding_code(finding: str) -> str:
    parts = finding.split(": ", 2)
    return parts[1] if len(parts) == 3 else ""


def report_classified_findings(findings: list[str], advisory_codes: frozenset[str]) -> int:
    advisory = [finding for finding in findings if finding_code(finding) in advisory_codes]
    hard = [finding for finding in findings if finding_code(finding) not in advisory_codes]
    unsafe = report_findings(advisory, True) if advisory else 0
    if hard:
        report_findings(hard, False)
    return 1 if hard or unsafe else 0

def report_findings(findings: list[str], warn_only: bool) -> int:
    hard = False
    for finding in findings:
        security = "escapes repository" in finding or "escapes the repository" in finding or "symlink-not-allowed" in finding
        hard = hard or security
        if warn_only and not security:
            # GitHub workflow-command escaping also prevents annotation injection.
            escaped = finding.replace('%', '%25').replace('\r', '%0D').replace('\n', '%0A')
            print(f'::warning::{escaped}')
        else:
            print(finding)
    if warn_only:
        print(f'document-content: WARN ({len(findings)} finding(s); advisory, not proof of conformance)')
        return 1 if hard else 0
    return 1
