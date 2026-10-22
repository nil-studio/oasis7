#!/usr/bin/env python3
"""Apply observation-only overlay to an exclusively owned pinned source checkout."""
import hashlib
from pathlib import Path

SOURCE = 'a1575b98a8cc19bcfa7bb6cb41be688cb3dae65c'
MAIN_SHA = 'd82b9405a76b782b70d9b063c02c7b18bab7130e88363bb8155d9498a0cf6684'


def patch_main(data):
    if hashlib.sha256(data).hexdigest() != MAIN_SHA:
        raise RuntimeError('pinned extractor main differs')
    text = data.decode()
    changes = [
        ('mod archive;\n', 'mod archive;\nmod inventory;\n'),
        ('        self.extract(&RustAnalyzer::from(err), file, source_kind);',
         '        inventory::without_semantics(file, source_kind == SourceKind::Library, err.reason);\n'
         '        self.extract(&RustAnalyzer::from(err), file, source_kind);'),
        ('    extractor.emit_extraction_diagnostics(start, &cfg)\n',
         '    inventory::finish_fallback();\n    extractor.emit_extraction_diagnostics(start, &cfg)\n'),
        ('            let semantics = Semantics::new(db);\n',
         '            let semantics = Semantics::new(db);\n'
         '            let mut inventory = (env::var("OASIS7_RUST_INVENTORY").as_deref() == Ok("1"))\n'
         '                .then(|| inventory::Inventory::new(&semantics, manifest.manifest_path().as_str()));\n'),
        ('            for file in files {\n                match extractor.load_source',
         '            for file in files {\n'
         '                if let Some(observer) = inventory.as_mut() {\n'
         '                    observer.observe(&semantics, vfs, path_to_file_id(file, vfs), file,\n'
         '                        "initial", true, None, false);\n'
         '                }\n                match extractor.load_source'),
        ('            for (file_id, file) in vfs.iter() {\n',
         '            for (file_id, file) in vfs.iter() {\n'
         '                if let Some(observer) = inventory.as_mut() {\n'
         '                    if let Some(path) = file.as_path().map(<_ as AsRef<Path>>::as_ref) {\n'
         '                        if path.extension().is_some_and(|ext| ext == "rs") {\n'
         '                            let processed = processed_files.contains(path);\n'
         '                            let library = (!processed).then(|| db.source_root(\n'
         '                                db.file_source_root(file_id).source_root_id(db)).source_root(db).is_library);\n'
         '                            observer.observe(&semantics, vfs, Some(file_id), path, "vfs",\n'
         '                                processed, library, library == Some(true));\n'
         '                        }\n'
         '                    }\n'
         '                }\n'),
        ('                    extractor.archiver.archive(file);\n                }\n            }\n        } else {',
         '                    extractor.archiver.archive(file);\n                }\n            }\n'
         '            if let Some(observer) = inventory { observer.finish(); }\n        } else {'),
    ]
    for old, new in changes:
        if text.count(old) != 1:
            raise RuntimeError('instrumentation anchor differs')
        text = text.replace(old, new, 1)
    restored = text
    for old, new in reversed(changes):
        if restored.count(new) != 1:
            raise RuntimeError('overlay is not uniquely reversible')
        restored = restored.replace(new, old, 1)
    if restored.encode() != data:
        raise RuntimeError('original extraction code changed')
    return text.encode()


def apply(source):
    source = Path(source)
    if not (source / '.oasis7-owned-inventory-source').is_file():
        raise RuntimeError('missing exclusive owned checkout marker')
    main = source / 'rust/extractor/src/main.rs'
    candidate = patch_main(main.read_bytes())
    module = main.with_name('inventory.rs')
    if module.exists():
        raise RuntimeError('candidate module already exists')
    module.write_bytes(Path(__file__).with_name('inventory.rs').read_bytes())
    main.write_bytes(candidate)
    return {'main_sha256': hashlib.sha256(candidate).hexdigest(),
            'inventory_sha256': hashlib.sha256(module.read_bytes()).hexdigest(),
            'changes': ['observation hooks only', 'new inventory module'],
            'extraction_selection': 'ORIGINAL_BYTES_UNCHANGED'}
