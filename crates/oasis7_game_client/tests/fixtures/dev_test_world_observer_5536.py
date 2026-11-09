"""Read-only independent canonical CAS observer for the bounded Linux fixture.
Requires cbor2, zstandard and blake3 in the trusted observer environment.
"""
import hashlib, io, json, pathlib, sys
import blake3, cbor2, zstandard
root=pathlib.Path(sys.argv[1])
accounts=json.loads((root/"read-5535-result.json").read_text())["accounts"]
records=sorted((p for p in (root/"preview/records").glob("*.json") if p.stem.isdigit()), key=lambda p:int(p.stem))
record=records[-1]
receipt=json.loads(record.read_text())
def blob(reference):
    encoded=(root/"preview/store/blobs"/(reference+".blob")).read_bytes()
    if encoded.startswith(b"O7CBLOB1"):
        assert len(encoded)>=80 and encoded[16:80].decode()==reference
        size=int.from_bytes(encoded[8:16],"little"); assert size<=16*1024*1024
        with zstandard.ZstdDecompressor().stream_reader(io.BytesIO(encoded[80:])) as stream:
            decoded=stream.read(size+1)
        assert len(decoded)==size
    else:
        decoded=encoded; assert len(decoded)<=16*1024*1024
    assert reference in [blake3.blake3(decoded).hexdigest(),hashlib.sha256(decoded).hexdigest()]
    return cbor2.loads(decoded)
snapshot=blob(receipt["snapshot_ref"])
journals={value["journal_ref"] for path in records if (value:=json.loads(path.read_text())).get("journal_ref")}
state=snapshot["state"]
subjects={account["fence"]["agent_id"] for account in accounts}
events={}
for reference in sorted(journals):
    for event in blob(reference)["events"]:
        payload=event.get("body",{}).get("payload",{})
        if payload.get("type")=="AgentMoved" and payload.get("data",{}).get("agent_id") in subjects:
            identity=json.dumps([snapshot["event_id_era"],event["id"]],sort_keys=True)
            assert identity not in events or events[identity]==event
            events[identity]=event
events=list(events.values())
result={"world_id":receipt["world_id"],"height":receipt["height"],"execution_block_hash":receipt["execution_block_hash"],"snapshot_ref":receipt["snapshot_ref"],"journal_ref":receipt["journal_ref"],"payload_hashes_verified":True,"positions":{agent:state["agents"][agent]["state"]["pos"] for agent in sorted(subjects)},"movement_events":events,"event_id_era":snapshot["event_id_era"],"last_event_id":snapshot["last_event_id"],"world_resources":state["resources"],"gameplay_policy":state["gameplay_policy"],"active_modules":snapshot["module_registry"]["active"]}
print(json.dumps(result))
