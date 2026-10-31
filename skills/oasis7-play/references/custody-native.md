# Custody native: closed Control-only profile

Use this reference only when the trusted launch input selects
`custody-native-v1` and supplies the source-matching client, this formal Skill,
one approved socket and one fixed resource reference. A claimed profile or a
readable descriptor does not establish Host/Model/Native isolation, actual Skill
loading, model-session ownership or provider cost limits. Missing or rejected
binding/isolation remains blocked; the model cannot choose another vault,
session, socket, origin or resource to bypass it.

## Current tool boundary

The current closed methods are `initialize`, `tools_list`, `heartbeat`,
`request_finish` and `request_abort`. The trusted Host retains signing material,
private inbox and purpose credentials; it prepares the fixed, authenticated
Control transports. The model receives none of those secrets and does not call
`custody-host-*`, enrollment/claim/replacement, raw signing, Native custody or
Owner endpoints. A lost Claim token requires the trusted operator's explicit
recovery; historical metadata is not a recoverable token.

Send only the supplied closed request on stdin:

```sh
<client> custody-control-call --socket <approved-custody-socket>
```

```json
{"api_version":1,"resource_id":"<approved-resource>","request_id":"<stable-control-id>","method":"initialize"}
```

Select a stable request ID once for that exact method/resource. Retain the same
request after an unknown reply; changing the method under an old ID conflicts.
The other closed methods use the same shape with their exact method name.
There are no `args`, arbitrary body, URL, token, purpose or private path fields.
Do not copy ordinary `--config` examples or the ordinary broker `args` shape
into this profile. Neither another profile's permissions nor its cached context
can supply missing custody authority.

`initialize` and `tools_list` acknowledge Control metadata only. They do not
return a private task, usable context or a business tool catalog. The current
profile does not expose task, capabilities, industry, context, quote, action,
outcome or original World-result lookup. Stop with a clear blocked report when
the goal needs them; do not infer actions or fall back to local keys/Native.

## Results and stopping

Replies contain `api_version`, the same `request_id`, a closed `status` and
`close_requested`. An `accepted` acknowledgement confirms that Control request;
`historical` refers to its retained acknowledgement. Neither proves task
activation, a world effect, Stop/Revoke or World Closed. `unknown`, `rejected`
and `unavailable` remain distinct failures/uncertainty, not a grant. Report the
fixed status and original safe reference; never print private bodies or secrets.

After `unknown`, an explicitly authorized retry uses the identical request,
within the original remaining deadline and budget. Each actual transport can
cost another attempt; no retry refunds, renews the deadline or increases a cap.
Do not loop, allocate replacement IDs to evade exhaustion, or restart to reset
state. The trusted daemon handles bounded heartbeats; do not poll or add manual
heartbeats unless the supplied run instructions explicitly require one.

On a stop instruction or unavailable execution, cease new game work. If the
supplied run authorizes it, use `request_finish` for intended completion or
`request_abort` for abandoning this run, preserving its exact original on an
unknown reply. `close_requested=true` means a closing request, not completed
cleanup. Owner Stop/Revoke, pending original reconciliation and resource cleanup
require their independent authority and evidence. Never invoke owner controls
or bootstrap to finish the run yourself.

## Evidence boundary

This reference describes a Control-only component, not a passed CT-008 or
ST-02/03/04/06 gameplay run. Record actual Skill/client versions and safe Control
references, then report game tools or isolation/launcher/lifecycle prerequisites
that are absent. Hashing this file does not prove the Model loaded it. Movement
alone cannot prove industrial production, and scripts/fixtures cannot stand in
for an independent Agent's decisions, canonical effects or bounded cleanup.
The ordinary prepared-world game profile remains separate and does not certify
this custody profile, dot/OAuth/MCP, cold entry or another Runtime.
