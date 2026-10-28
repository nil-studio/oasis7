//! Local filesystem streaming fixture; not a durable-domain or crash test.
use super::*;
use std::cell::Cell;
use std::fs::File;
use std::path::PathBuf;

struct Files {
    path: PathBuf,
    memory: MemoryBudget,
    peak_charge: Cell<usize>,
    reads: Cell<usize>,
}

impl Files {
    fn new(memory: MemoryBudget) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("oasis7-iws-value-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self {
            path,
            memory,
            peak_charge: Cell::new(0),
            reads: Cell::new(0),
        }
    }

    fn sample(&self) {
        self.peak_charge
            .set(self.peak_charge.get().max(self.memory.used()));
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}

impl ValueNodeWriter for Files {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(bytes.uses_budget(&self.memory));
        self.sample();
        std::fs::write(self.path.join(hex::encode(id)), bytes.as_slice())?;
        Ok(())
    }
}

impl ValueNodeReader for Files {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let mut file = File::open(self.path.join(hex::encode(id)))?;
        let length = usize::try_from(file.metadata()?.len())
            .map_err(|_| ValueTreeError::Invalid("file length overflow"))?;
        if length > max_bytes {
            return Err(ValueTreeError::Invalid("file page bound"));
        }
        let charge = memory.reserve(length, class)?;
        self.sample();
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes)?;
        self.reads.set(self.reads.get() + 1);
        Ok(Arc::new(charge.retain(bytes)))
    }
}

struct Pattern {
    position: u64,
}
impl Read for Pattern {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        for byte in out.iter_mut() {
            *byte = (self.position % 251) as u8;
            self.position += 1;
        }
        Ok(out.len())
    }
}

#[test]
fn value_above_old_sixty_four_mib_limit_streams_under_one_mib_of_reservations() {
    let memory = MemoryBudget::new(1024 * 1024, 256 * 1024).unwrap();
    let mut files = Files::new(memory.clone());
    let mut input = Pattern { position: 0 };
    let size = 64 * 1024 * 1024 + 17;
    let mut quota = ValueIoBudget {
        nodes_remaining: 2048,
        read_bytes_remaining: size,
        write_bytes_remaining: size + 1024 * 1024,
    };
    let root = write_value(&mut input, size, &mut files, &memory, &mut quota, || false).unwrap();
    assert_eq!(input.position, size);
    assert_eq!(root.0.depth, 2);
    assert_eq!(memory.used(), 0);
    assert!(files.peak_charge.get() <= VALUE_BUILD_WORKSPACE_BYTES + VALUE_MAX_NODE_BYTES);
    assert_eq!(
        std::fs::read(files.path.join(hex::encode(root.digest().unwrap()))).unwrap(),
        root.canonical_bytes().unwrap()
    );

    // Cross the old boundary and a top-level directory boundary at once.
    let offset = 64 * 1024 * 1024 - 4;
    let mut quota = ValueIoBudget {
        nodes_remaining: 5,
        read_bytes_remaining: 2 * VALUE_MAX_NODE_BYTES as u64 + 16000,
        write_bytes_remaining: 12,
    };
    let ordinary_load = memory.reserve(768 * 1024, MemoryClass::Ordinary).unwrap();
    let output_charge = memory.reserve(12, MemoryClass::Recovery).unwrap();
    let mut output = Vec::with_capacity(12);
    ValueReadContext {
        source: &files,
        memory: &memory,
        class: MemoryClass::Recovery,
        io: &mut quota,
    }
    .read_range(&root, offset, 12, &mut output, || false)
    .unwrap();
    assert_eq!(
        output,
        (offset..offset + 12)
            .map(|n| (n % 251) as u8)
            .collect::<Vec<_>>()
    );
    assert_eq!(files.reads.get(), 5);
    drop(output);
    drop(output_charge);
    drop(ordinary_load);
    assert_eq!(memory.used(), 0);

    // Read failure remains an IO error, with the original tree untouched.
    let path = files.path.join(hex::encode(root.0.id));
    let index_bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let mut quota = ValueIoBudget {
        nodes_remaining: 5,
        read_bytes_remaining: 100000,
        write_bytes_remaining: 12,
    };
    assert!(matches!(
        ValueReadContext {
            source: &files,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut quota
        }
        .read_range(&root, offset, 12, &mut std::io::sink(), || false),
        Err(ValueTreeError::Io(_))
    ));
    std::fs::write(path, index_bytes).unwrap();
    assert_eq!(memory.used(), 0);
}
