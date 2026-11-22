fn text(s: &str) -> Vec<u8> {
    let mut b = vec![1];
    b.extend_from_slice(&(s.len() as u64).to_be_bytes());
    b.extend(s.as_bytes());
    b
}
fn pair(a: Vec<u8>, b: Vec<u8>) -> Vec<u8> {
    let mut out = vec![5];
    out.extend(a);
    out.extend(b);
    out
}
pub(super) enum Path<'a> {
    Record,
    Field(&'a str),
    Crisis(&'a str),
    Flag(&'a str),
    Value(&'a str),
    Meta(&'a str),
    Member(&'a str, &'a str),
    Frozen(&'a str, &'a str),
}
pub(super) fn semantic(path: Path<'_>) -> Vec<u8> {
    let unit = vec![0];
    let crisis = || pair(vec![0], text("crisis_evidence"));
    let body = match path {
        Path::Record => unit,
        Path::Field(n) => pair(unit, text(n)),
        Path::Crisis(n) => pair(crisis(), text(n)),
        Path::Flag(n) => pair(pair(crisis(), text(n)), vec![0]),
        Path::Value(n) => pair(pair(crisis(), text(n)), text("value")),
        Path::Meta(n) => pair(pair(unit, text(n)), vec![0]),
        Path::Member(c, k) => pair(pair(unit, text(c)), text(k)),
        Path::Frozen(k, n) => pair(pair(pair(unit, text("frozen_lineages")), text(k)), text(n)),
    };
    let mut out = b"oasis7.iws.semantic-key.v1\0".to_vec();
    out.extend(body);
    out
}
