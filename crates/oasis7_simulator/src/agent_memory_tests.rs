use super::*;

#[test]
fn short_term_memory_basic() {
    let mut memory = ShortTermMemory::new(3);
    memory.add(MemoryEntry::observation(10, "hello"));
    assert_eq!(memory.len(), 1);
    assert_eq!(memory.all().count(), 1);
}

#[test]
fn short_term_memory_capacity_eviction() {
    let mut memory = ShortTermMemory::new(2);
    memory.add(MemoryEntry::observation(10, "a"));
    memory.add(MemoryEntry::observation(11, "b"));
    memory.add(MemoryEntry::observation(12, "c"));

    assert_eq!(memory.len(), 2);
    let times: Vec<_> = memory.all().map(|entry| entry.time).collect();
    assert_eq!(times, vec![11, 12]);
}

#[test]
fn short_term_memory_since_filter() {
    let mut memory = ShortTermMemory::new(3);
    memory.add(MemoryEntry::observation(10, "a"));
    memory.add(MemoryEntry::observation(20, "b"));

    let recent: Vec<_> = memory.since(15).collect();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].time, 20);
}

#[test]
fn short_term_memory_importance_filter() {
    let mut memory = ShortTermMemory::new(3);
    memory.add(MemoryEntry::observation(10, "a").with_importance(0.1));
    memory.add(MemoryEntry::observation(20, "b").with_importance(0.9));

    let important: Vec<_> = memory.important(0.5).collect();
    assert_eq!(important.len(), 1);
    assert_eq!(important[0].time, 20);
}

#[test]
fn short_term_memory_summarize() {
    let mut memory = ShortTermMemory::new(3);
    memory.add(MemoryEntry::observation(10, "a"));
    memory.add(MemoryEntry::event(20, "storm"));

    let summary = memory.summarize(5);
    assert!(summary.contains("a"));
    assert!(summary.contains("storm"));
}

#[test]
fn long_term_memory_basic() {
    let mut memory = LongTermMemory::new();
    let id = memory.store("hello", 10);
    assert_eq!(memory.len(), 1);
    let entry = memory.get(&id, 11).unwrap();
    assert_eq!(entry.content, "hello");
}

#[test]
fn long_term_memory_search_by_tag() {
    let mut memory = LongTermMemory::new();
    memory.store_with_tags("a", 10, vec!["tag-1".to_string()]);
    memory.store_with_tags("b", 11, vec!["tag-2".to_string()]);

    let results = memory.search_by_tag("tag-1");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].content, "a");
}

#[test]
fn long_term_memory_search_by_content() {
    let mut memory = LongTermMemory::new();
    memory.store_with_tags("hello", 10, vec!["tag-1".to_string()]);
    memory.store_with_tags("world", 11, vec!["tag-2".to_string()]);

    let results = memory.search_by_content("world");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].content, "world");
}

#[test]
fn long_term_memory_capacity_eviction() {
    let mut memory = LongTermMemory::with_capacity(2);
    let id_a = memory.store("a", 10);
    let _id_b = memory.store("b", 11);
    let _id_c = memory.store("c", 12);

    assert_eq!(memory.len(), 2);
    assert!(!memory.entries.contains_key(&id_a));
}

#[test]
fn long_term_memory_top_by_importance() {
    let mut memory = LongTermMemory::new();
    let id_low = memory.store("a", 10);
    let id_high = memory.store("b", 11);
    memory.entries.get_mut(&id_low).unwrap().importance = 0.1;
    memory.entries.get_mut(&id_high).unwrap().importance = 0.9;

    let top = memory.top_by_importance(1);
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].content, "b");
}

#[test]
fn long_term_memory_top_by_importance_preserves_tie_order() {
    let mut memory = LongTermMemory::new();
    let id_a = memory.store("a", 10);
    let id_b = memory.store("b", 11);
    let id_c = memory.store("c", 12);
    memory.entries.get_mut(&id_a).unwrap().importance = 0.7;
    memory.entries.get_mut(&id_b).unwrap().importance = 0.7;
    memory.entries.get_mut(&id_c).unwrap().importance = 0.1;

    let top = memory.top_by_importance(2);
    let contents: Vec<_> = top.iter().map(|entry| entry.content.as_str()).collect();
    assert_eq!(contents, vec!["a", "b"]);
}

#[test]
fn long_term_memory_recently_accessed_limits_and_preserves_tie_order() {
    let mut memory = LongTermMemory::new();
    let id_a = memory.store("a", 10);
    let id_b = memory.store("b", 11);
    let id_c = memory.store("c", 12);
    memory.entries.get_mut(&id_a).unwrap().last_accessed = 30;
    memory.entries.get_mut(&id_b).unwrap().last_accessed = 40;
    memory.entries.get_mut(&id_c).unwrap().last_accessed = 40;

    let recent = memory.recently_accessed(2);
    let contents: Vec<_> = recent.iter().map(|entry| entry.content.as_str()).collect();
    assert_eq!(contents, vec!["b", "c"]);
}

#[test]
fn long_term_memory_export_restore_roundtrip_preserves_entries_and_next_id() {
    let mut source = LongTermMemory::new();
    source.store_with_tags("first", 10, vec!["tag-a".to_string()]);
    source.store_with_tags("second", 11, vec!["tag-b".to_string()]);
    let exported = source.export_entries();
    assert_eq!(exported.len(), 2);

    let mut restored = LongTermMemory::new();
    restored.restore_entries(exported);
    assert_eq!(restored.len(), 2);
    assert_eq!(restored.search_by_tag("tag-a").len(), 1);
    assert_eq!(restored.search_by_tag("tag-b").len(), 1);

    let next_id = restored.store("third", 12);
    assert_eq!(next_id, "mem-2");
}

#[test]
fn agent_memory_combined() {
    let mut memory = AgentMemory::with_capacities(2, 2);
    memory.record_observation(10, "a");
    memory.long_term.store("b", 11);

    assert_eq!(memory.short_term.len(), 1);
    assert_eq!(memory.long_term.len(), 1);
}

#[test]
fn agent_memory_consolidation() {
    let mut memory = AgentMemory::with_capacities(2, 2);
    memory.record_observation(10, "a");

    memory.consolidate(11, 0.4);
    assert_eq!(memory.short_term.len(), 1);
    assert_eq!(memory.long_term.len(), 1);
}

#[test]
fn memory_entry_serialization() {
    let entry = MemoryEntry::note(10, "hello");
    let serialized = serde_json::to_string(&entry).unwrap();
    assert!(serialized.contains("hello"));
}

#[test]
fn agent_memory_restores_long_term_entries() {
    let mut memory = AgentMemory::with_capacities(8, 8);
    let entry = LongTermMemoryEntry::new("mem-7", 20, "persisted note").with_tag("persisted");
    memory.restore_long_term_entries(vec![entry]);

    let exported = memory.export_long_term_entries();
    assert_eq!(exported.len(), 1);
    assert_eq!(exported[0].id, "mem-7");
    assert_eq!(exported[0].content, "persisted note");

    let next_id = memory.long_term.store("runtime", 21);
    assert_eq!(next_id, "mem-8");
}
