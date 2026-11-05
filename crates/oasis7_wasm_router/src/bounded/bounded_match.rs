use super::bounded_json::{Arena, Kind, step};
use super::{BoundedRouterErrorV1 as Error, RouterWorkV1};
use oasis7_wasm_abi::{ModuleSubscription, ModuleSubscriptionStage};
use serde_json::Value;
struct Matcher<'a, R, C> {
    a: &'a Arena<R>,
    w: &'a mut RouterWorkV1,
    c: &'a mut C,
}
impl<R, C: FnMut() -> bool> Matcher<'_, R, C> {
    fn tick<E>(&mut self) -> Result<(), Error<E>> {
        step(self.w, self.c).map_err(Error::from_internal)
    }
    fn same<E>(&mut self, a: &[u8], b: &[u8]) -> Result<bool, Error<E>> {
        self.tick()?;
        if a.len() != b.len() {
            return Ok(false);
        }
        for (x, y) in a.iter().zip(b) {
            self.tick()?;
            if x != y {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn eq<E>(&mut self, id: usize, v: &Value, depth: usize) -> Result<bool, Error<E>> {
        self.tick()?;
        if depth > self.w.max_depth {
            return Err(Error::ResourceLimited("filter depth"));
        }
        match (&self.a.nodes[id].kind, v) {
            (Kind::Null, Value::Null) => Ok(true),
            (Kind::Bool(a), Value::Bool(b)) => Ok(a == b),
            (Kind::Number(a), Value::Number(b)) => Ok(a == b),
            (Kind::String(r), Value::String(s)) => {
                self.same(&self.a.strings[r.clone()], s.as_bytes())
            }
            (Kind::Array, Value::Array(values)) => {
                let mut edge = self.a.nodes[id].first;
                for value in values {
                    let Some(e) = edge else { return Ok(false) };
                    if !self.eq(self.a.edges[e].child, value, depth + 1)? {
                        return Ok(false);
                    }
                    edge = self.a.edges[e].next;
                }
                Ok(edge.is_none())
            }
            (Kind::Object, Value::Object(values)) => {
                let mut count = 0;
                let mut edge = self.a.nodes[id].first;
                while let Some(e) = edge {
                    self.tick()?;
                    count += 1;
                    edge = self.a.edges[e].next;
                }
                if count != values.len() {
                    return Ok(false);
                }
                for (k, v) in values {
                    let mut edge = self.a.nodes[id].first;
                    let mut found = None;
                    while let Some(e) = edge {
                        self.tick()?;
                        let key = self.a.edges[e]
                            .key
                            .as_ref()
                            .ok_or(Error::Invalid("object key"))?;
                        if self.same(&self.a.strings[key.clone()], k.as_bytes())? {
                            found = Some(self.a.edges[e].child);
                            break;
                        }
                        edge = self.a.edges[e].next;
                    }
                    let Some(child) = found else { return Ok(false) };
                    if !self.eq(child, v, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn token<E>(&mut self, raw: &str, key: &[u8]) -> Result<bool, Error<E>> {
        let mut bytes = raw.bytes();
        let mut i = 0;
        while let Some(b) = bytes.next() {
            self.tick()?;
            let decoded = if b == b'~' {
                match bytes.next() {
                    Some(b'0') => b'~',
                    Some(b'1') => b'/',
                    _ => return Err(Error::Invalid("pointer escape")),
                }
            } else {
                b
            };
            if key.get(i) != Some(&decoded) {
                return Ok(false);
            }
            i += 1;
        }
        Ok(i == key.len())
    }
    fn pointer<E>(&mut self, path: &str) -> Result<Option<usize>, Error<E>> {
        if path.is_empty() {
            return Ok(Some(0));
        }
        if !path.starts_with('/') {
            return Err(Error::Invalid("pointer"));
        }
        let mut id = 0;
        for token in path[1..].split('/') {
            self.tick()?;
            match self.a.nodes[id].kind {
                Kind::Object => {
                    let mut edge = self.a.nodes[id].first;
                    let mut found = None;
                    while let Some(e) = edge {
                        self.tick()?;
                        let key = self.a.edges[e].key.as_ref().ok_or(Error::Invalid("key"))?;
                        if self.token(token, &self.a.strings[key.clone()])? {
                            found = Some(self.a.edges[e].child);
                            break;
                        }
                        edge = self.a.edges[e].next;
                    }
                    let Some(next) = found else { return Ok(None) };
                    id = next
                }
                Kind::Array => {
                    for _ in token.bytes() {
                        self.tick()?
                    }
                    if token.is_empty()
                        || (token.starts_with('0') && token.len() > 1)
                        || !token.bytes().all(|b| b.is_ascii_digit())
                    {
                        return Ok(None);
                    }
                    let Ok(index) = token.parse::<usize>() else {
                        return Ok(None);
                    };
                    let mut edge = self.a.nodes[id].first;
                    for _ in 0..index {
                        self.tick()?;
                        let Some(e) = edge else { return Ok(None) };
                        edge = self.a.edges[e].next;
                    }
                    let Some(e) = edge else { return Ok(None) };
                    id = self.a.edges[e].child;
                }
                _ => return Ok(None),
            }
        }
        Ok(Some(id))
    }
    fn rule<E>(&mut self, v: &Value, validate: bool) -> Result<bool, Error<E>> {
        self.tick()?;
        let o = v.as_object().ok_or(Error::Invalid("rule"))?;
        let path = o
            .get("path")
            .and_then(Value::as_str)
            .ok_or(Error::Invalid("path"))?;
        let mut op = None;
        for (k, value) in o {
            self.tick()?;
            if k == "path" {
                continue;
            }
            if !matches!(k.as_str(), "eq" | "ne" | "gt" | "gte" | "lt" | "lte" | "re")
                || op.is_some()
            {
                return Err(Error::Invalid("operator"));
            }
            if k == "re" {
                return Err(Error::Unsupported("strict router regex"));
            }
            if !matches!(k.as_str(), "eq" | "ne") && value.as_f64().is_none() {
                return Err(Error::Invalid("numeric operator"));
            }
            op = Some((k.as_str(), value));
        }
        let (op, value) = op.ok_or(Error::Invalid("operator"))?;
        if !path.is_empty() && !path.starts_with('/') {
            return Err(Error::Invalid("pointer"));
        }
        let mut chars = path.bytes();
        while let Some(ch) = chars.next() {
            self.tick()?;
            if ch == b'~' && !matches!(chars.next(), Some(b'0' | b'1')) {
                return Err(Error::Invalid("pointer escape"));
            }
        }
        if validate {
            return Ok(true);
        }
        let Some(id) = self.pointer(path)? else {
            return Ok(false);
        };
        if op == "eq" || op == "ne" {
            let same = self.eq(id, value, 0)?;
            return Ok(if op == "eq" { same } else { !same });
        }
        let Kind::Number(n) = &self.a.nodes[id].kind else {
            return Ok(false);
        };
        let Some(n) = n.as_f64() else {
            return Ok(false);
        };
        let t = value.as_f64().ok_or(Error::Invalid("number"))?;
        Ok(match op {
            "gt" => n > t,
            "gte" => n >= t,
            "lt" => n < t,
            _ => n <= t,
        })
    }
    fn rules<E>(&mut self, v: &Value, validate: bool) -> Result<bool, Error<E>> {
        self.tick()?;
        if let Some(list) = v.as_array() {
            let mut yes = true;
            for rule in list {
                let matched = self.rule(rule, validate)?;
                yes &= matched;
            }
            return Ok(yes);
        }
        let o = v.as_object().ok_or(Error::Invalid("rules"))?;
        let mut all = true;
        let mut any = false;
        let mut has_any = false;
        for (k, v) in o {
            self.tick()?;
            if k != "all" && k != "any" {
                return Err(Error::Invalid("rule group"));
            }
            let list = v.as_array().ok_or(Error::Invalid("rule list"))?;
            for r in list {
                let yes = self.rule(r, validate)?;
                if k == "all" {
                    all &= yes
                } else {
                    has_any = true;
                    any |= yes
                }
            }
        }
        Ok(all && (!has_any || any))
    }
}
pub(super) fn pre_action_matches<R, C: FnMut() -> bool, E>(
    a: &Arena<R>,
    subscriptions: &[ModuleSubscription],
    _module_id: &str,
    action_kind: &str,
    w: &mut RouterWorkV1,
    c: &mut C,
) -> Result<bool, Error<E>> {
    let mut m = Matcher { a, w, c };
    // Match the legacy prepare_subscriptions admission contract for the entire manifest.
    for s in subscriptions {
        m.tick()?;
        let events = !s.event_kinds.is_empty();
        let actions = !s.action_kinds.is_empty();
        let invalid = match s.stage {
            Some(ModuleSubscriptionStage::PostEvent) => actions || !events,
            Some(ModuleSubscriptionStage::PreAction | ModuleSubscriptionStage::PostAction) => {
                events || !actions
            }
            Some(ModuleSubscriptionStage::Tick) => {
                events || actions || s.filters.as_ref().is_some_and(|v| !v.is_null())
            }
            None => events == actions,
        };
        if invalid {
            return Err(Error::Invalid("subscription stage"));
        }
        if let Some(f) = s.filters.as_ref().filter(|v| !v.is_null()) {
            let obj = f.as_object().ok_or(Error::Invalid("filters"))?;
            for (k, r) in obj {
                m.tick()?;
                if k != "event" && k != "action" {
                    return Err(Error::Invalid("filter kind"));
                }
                if !r.is_null() {
                    m.rules(r, true)?;
                }
            }
        }
    }
    let mut matched = false;
    for s in subscriptions {
        m.tick()?;
        if s.resolved_stage() != ModuleSubscriptionStage::PreAction {
            continue;
        }
        let mut kinds = false;
        for pattern in &s.action_kinds {
            for _ in pattern.bytes().chain(action_kind.bytes()) {
                m.tick()?
            }
            kinds |= super::super::filtering::subscription_match(pattern, action_kind)
        }
        if kinds {
            matched |= match s
                .filters
                .as_ref()
                .and_then(|f| f.get("action"))
                .filter(|v| !v.is_null())
            {
                Some(r) => m.rules(r, false)?,
                None => true,
            };
        }
    }
    Ok(matched)
}
