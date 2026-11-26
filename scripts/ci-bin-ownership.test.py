#!/usr/bin/env python3
"""Executable ownership and lightweight service dependency contracts."""
from pathlib import Path
import json
import re
import tomllib
import unittest

ROOT = Path(__file__).resolve().parent.parent
OWNERS = {
    'oasis7_newapi_bridge_service': ['oasis7_newapi_bridge_service'],
    'oasis7_provider_local_bridge': ['oasis7_provider_local_bridge'],
    'oasis7_provider_tools': ['oasis7_llm_agent_demo', 'oasis7_llm_provider_probe', 'oasis7_provider_parity_bench'],
    'oasis7_launcher_backend': ['oasis7_game_launcher', 'oasis7_web_launcher'],
    'oasis7_client_cli': ['oasis7_pure_api_client'],
    'oasis7_protocol_tools': ['oasis7_chain_transfer_submit_client', 'oasis7_rollback_strict_audit_evidence'],
    'oasis7_simulator': ['oasis7_init_demo'],
    'oasis7_viewer_runtime': ['oasis7_viewer_live', 'oasis7_viewer_server', 'oasis7_viewer_demo'],
    'oasis7_ops_tools': ['oasis7_generate_world_artifact', 'oasis7_governance_registry_audit',
                        'oasis7_governance_registry_import',
                        'oasis7_liveops_grant_cli',
                        'oasis7_runtime_module_routing_perf', 'oasis7_testnet_faucet',
                        'oasis7_world_repair_rebuild'],
}


def manifest(package):
    return tomllib.loads((ROOT / 'crates' / package / 'Cargo.toml').read_text())


def flat_rust_imports(source, crate_path):
    """Read direct, unaliased members of one Rust import path."""
    pattern = '^use ' + re.escape(crate_path) + r'::(?:([A-Za-z_]\w*)|\{([^{};]*)\});'
    return {
        member.strip()
        for direct, group in re.findall(pattern, source, re.MULTILINE)
        for member in ([direct] if direct else group.split(','))
        if re.fullmatch(r'[A-Za-z_]\w*', member.strip())
    }


def require_materialized_native_archive(root):
    """The trusted gate requires real L4 archive behavior and selected facades."""
    native = root / 'crates/oasis7_runtime_host/src/simulator_archive'
    facade = root / 'crates/oasis7/src/simulator'

    def require(path, patterns):
        source = path.read_text()
        for pattern in patterns:
            if re.search(pattern, source, re.MULTILINE) is None:
                raise AssertionError(f'incomplete native archive source: {path}: {pattern}')
        return source

    require(native / 'mod.rs', (
        r'^pub mod archive;$', r'^pub mod kernel_host;$', r'^pub mod persist;$',
    ))
    require(native / 'kernel_host.rs', (
        r'^pub struct WorldKernel \{$', r'^\s*kernel: NativeKernel,',
        r'^\s*runtime_events: .*RuntimeWorldEvent',
        r'^\s*archive_origin: .*ArchiveOrigin',
        r'^impl WorldKernel \{$', r'^\s*fn assert_journal_sidecar\(',
        r'^\s*pub fn snapshot\(', r'^\s*pub fn journal_snapshot\(',
        r'^\s*pub fn replay_from_snapshot\(',
        r'^\s*pub fn save_to_dir\(', r'^\s*pub fn load_from_dir\(',
    ))
    require(native / 'persist.rs', (
        r'^pub struct WorldSnapshot \{$', r'^pub struct WorldJournal \{$',
        r'^\s*pub runtime_snapshot: Option<RuntimeSnapshot>,',
        r'^\s*pub player_gameplay: Option<PlayerGameplaySnapshot>,',
        r'^\s*pub chain_resource_manifest: ChainResourceManifest,',
        r'^impl WorldSnapshot \{$', r'^impl WorldJournal \{$',
    ))
    require(native / 'archive.rs', (
        r'^pub struct WorldEvent \{$',
        r'^\s*pub runtime_event: Option<RuntimeWorldEvent>,',
        r'^pub struct WorldSnapshotExtensions \{$',
        r'^impl WorldSnapshot \{$', r'^\s*pub fn into_kernel_parts\(',
        r'^\s*pub fn from_kernel_parts\(',
    ))
    for name, patterns in (
        ('archive_origin.rs', (r'ArchiveOrigin \{',)),
        ('codec.rs', (r'^impl Serialize for WorldKernel \{$',)),
        ('forwarding.rs', (r'^impl WorldKernel \{$', r'self\.kernel\.set_module_source_compiler')),
        ('journal_view.rs', (r'WorldJournalView',)),
    ):
        require(native / 'kernel_host' / name, patterns)

    selected = (facade / 'mod.rs').read_text()
    for name, module, visibility in (
        ('archive_host_facade.rs', 'archive', 'pub '),
        ('persist_host_facade.rs', 'persist', r'pub\(crate\) '),
        ('kernel_host_host_facade.rs', 'kernel_host', ''),
    ):
        native_decl = (r'#\[cfg\(not\(target_arch = "wasm32"\)\)\]\s*'
                       + re.escape(f'#[path = "{name}"]')
                       + r'\s*' + visibility + r'mod ' + module + r';')
        wasm_decl = (r'#\[cfg\(target_arch = "wasm32"\)\]\s*'
                     + visibility + r'mod ' + module + r';')
        if re.search(native_decl, selected) is None:
            raise AssertionError(f'native archive facade not selected: {module}')
        if re.search(wasm_decl, selected) is None:
            raise AssertionError(f'original wasm archive removed: {module}')
        require(facade / name, (
            r'^pub use oasis7_runtime_host::simulator_archive::' + module + r'::\*;$',
        ))
    require(facade / 'archive_host_facade.rs', (
        r'^#\[path = "archive/tests.rs"\]$',
        r'^#\[path = "archive/retention_tests.rs"\]$',
    ))


class Ownership(unittest.TestCase):
    def test_chain_and_viewer_observability_use_native_owner(self):
        """The actual process startup and diagnostics must share one native owner."""
        native = ROOT / 'crates/oasis7_observability/src/lib.rs'
        source = native.read_text()
        for function in ('init_tracing', 'emit_stderr_or_event', 'resolve_trace_session_id'):
            self.assertIn(f'pub fn {function}(', source)
        for package in ('oasis7_chain_runtime', 'oasis7_viewer_runtime'):
            deps = manifest(package)['target']['cfg(not(target_arch = "wasm32"))']['dependencies']
            self.assertEqual(deps['oasis7_observability'], {'path': '../oasis7_observability'})
            for rust in (ROOT / 'crates' / package / 'src').rglob('*.rs'):
                self.assertNotIn('oasis7::observability::', rust.read_text(), str(rust))
        for package, relative, function in (
            ('oasis7_chain_runtime', 'host.rs', 'resolve_trace_session_id'),
            ('oasis7_chain_runtime', 'host/execution_bridge/driver_observability.rs', 'emit_stderr_or_event'),
            ('oasis7_viewer_runtime', 'bin/oasis7_viewer_live.rs', 'resolve_trace_session_id'),
            ('oasis7_viewer_runtime', 'bin/oasis7_viewer_server.rs', 'init_tracing'),
            ('oasis7_viewer_runtime', 'web_bridge.rs', 'emit_stderr_or_event'),
        ):
            source = (ROOT / 'crates' / package / 'src' / relative).read_text()
            self.assertIn(f'oasis7_observability::{function}', source, relative)

    def test_native_viewer_value_types_and_retired_service_consumers(self):
        """Seal native Service consumers, preserve the composite historical kernel."""
        deps = manifest('oasis7_viewer_codec')['dependencies']
        self.assertEqual(deps['oasis7_simulator'], {'path': '../oasis7_simulator'})
        codec = (ROOT / 'crates/oasis7_viewer_codec/src/lib.rs').read_text()
        self.assertIn('use oasis7_simulator::{AgentDecisionTrace, RunnerMetrics, WorldTime};', codec)
        self.assertNotIn('oasis7::simulator::WorldTime', codec)
        for relative in (
            'crates/oasis7_viewer_codec/examples/codec_cost.rs',
            'crates/oasis7_viewer_runtime/src/lib.rs',
            'crates/oasis7_client_cli/src/bin/oasis7_pure_api_client/canonical_chat_fixture.rs',
        ):
            source = (ROOT / relative).read_text()
            self.assertNotIn('oasis7::GeoPos', source, relative)
            self.assertIn('oasis7_simulator::GeoPos' if relative.endswith('/lib.rs')
                          or relative.endswith('canonical_chat_fixture.rs') else 'GeoPos {',
                          source, relative)
        example = (ROOT / 'crates/oasis7_viewer_codec/examples/codec_cost.rs').read_text()
        self.assertIn('use oasis7::simulator::{WorldEvent, WorldKernel};', example)
        self.assertIn('use oasis7_simulator::{Agent, GeoPos, RejectReason, WorldEventKind};', example)
        # The composite old archive WorldKernel/WorldSnapshot is intentionally
        # not replaced by the non-equivalent pure Simulator kernel.
        for package in (ROOT / 'crates').iterdir():
            if not package.is_dir() or package.name == 'oasis7':
                continue
            for source_path in package.rglob('*.rs'):
                text = source_path.read_text()
                self.assertNotIn('oasis7::world_service::', text, str(source_path))

    def require_native_viewer_codec_archive_owner(self, source, manifest_data):
        """Keep the real native archive on L4 without importing native Host in wasm."""
        self.assertIn(
            '#[cfg(target_arch = "wasm32")]\n'
            'use oasis7::simulator::{WorldEvent, WorldSnapshot};',
            source,
        )
        self.assertIn(
            '#[cfg(not(target_arch = "wasm32"))]\n'
            'use oasis7_runtime_host::simulator_archive::archive::WorldEvent;',
            source,
        )
        self.assertIn(
            '#[cfg(not(target_arch = "wasm32"))]\n'
            'use oasis7_runtime_host::simulator_archive::persist::WorldSnapshot;',
            source,
        )
        self.assertEqual(
            manifest_data['target']['cfg(not(target_arch = "wasm32"))']['dependencies']['oasis7_runtime_host'],
            {'path': '../oasis7_runtime_host'},
        )
        self.assertNotIn('use oasis7_simulator::{WorldEvent, WorldSnapshot};', source)

    def test_native_viewer_codec_archive_owner_positive_and_negative(self):
        """Exact actual native owner, original wasm compatibility, and partial-migration rejection."""
        codec = (ROOT / 'crates/oasis7_viewer_codec/src/lib.rs').read_text()
        manifest_data = manifest('oasis7_viewer_codec')
        self.require_native_viewer_codec_archive_owner(codec, manifest_data)
        for wrong in (
            codec.replace(
                'oasis7_runtime_host::simulator_archive::archive::WorldEvent',
                'oasis7_simulator::WorldEvent',
            ),
            codec.replace(
                'oasis7_runtime_host::simulator_archive::persist::WorldSnapshot',
                'oasis7_simulator::WorldSnapshot',
            ),
            codec.replace(
                '#[cfg(target_arch = "wasm32")]',
                '#[cfg(not(target_arch = "wasm32"))]',
                1,
            ),
        ):
            with self.assertRaises(AssertionError):
                self.require_native_viewer_codec_archive_owner(wrong, manifest_data)
        removed = {
            **manifest_data,
            'target': {
                'cfg(not(target_arch = "wasm32"))': {'dependencies': {}},
            },
        }
        with self.assertRaises(KeyError):
            self.require_native_viewer_codec_archive_owner(codec, removed)

    def test_native_runtime_host_is_registered_in_required_test_drivers(self):
        """Archive and migrated host behavior must run as the real package, not only a dependency."""
        import importlib.util
        import subprocess
        import sys
        owner = 'oasis7_runtime_host'
        cfg = json.loads((ROOT / 'scripts/ci-required-scope.json').read_text())
        self.assertIn(owner, cfg['oasis7_required_packages'])
        features = manifest(owner)['features']
        self.assertIn('test_tier_required', features)
        self.assertIn('test_tier_required', features['test_tier_full'])
        shell = (ROOT / 'scripts/ci-tests.sh').read_text()
        functions = dict(re.findall(r'^(\w+)\(\) \{\n(.*?)^\}', shell, re.M | re.S))
        for fn_name, cargo_fn, invocation in (
            ('run_oasis7_package_tests', 'run_cargo', 'run_oasis7_package_tests test_tier_required --tests'),
            ('run_oasis7_required_tier_clippy', 'run_cargo_clippy', 'run_oasis7_required_tier_clippy'),
        ):
            script = (
                'set -euo pipefail\n'
                f'ci_python="{sys.executable}"\n'
                f'driver_dir="{ROOT / "scripts"}"\n'
                'required_plan=""\n'
                f'{cargo_fn}() {{ printf "%s\\n" "$*"; }}\n'
                f'{fn_name}() {{\n{functions[fn_name]}}}\n'
                f'{invocation}\n'
            )
            result = subprocess.run(['bash', '-c', script], text=True, capture_output=True, check=True)
            lines = result.stdout.splitlines()
            self.assertEqual(len(lines), 1, fn_name)
            self.assertIn(f'-p {owner} ', lines[0], fn_name)
        spec = importlib.util.spec_from_file_location('ci_test_artifacts', ROOT / 'scripts/ci-test-artifacts.py')
        artifacts = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(artifacts)
        required = next(args for name, args, _ in artifacts.build_variants('ordinary') if name == 'required')
        required_packages = [required[index + 1] for index, value in enumerate(required) if value == '-p']
        self.assertEqual(required_packages, cfg['oasis7_required_packages'])
        self.assertIn(owner, required_packages)

    def test_provider_original_material_contract_has_portable_owner(self):
        """Portable pending-root DTOs have one owner; native service keeps the codec."""
        native = (ROOT / 'crates/oasis7_formal_protocol/src/provider_material.rs').read_text()
        service = (ROOT / 'crates/oasis7_world_service/src/provider_material.rs').read_text()
        self.assertIn('pub use oasis7_formal_protocol::provider_material::*;', service)
        self.assertNotIn('pub struct ProviderMaterialOwnerBindingV1', service)
        self.assertIn('pub struct ProviderMaterialOwnerBindingV1', native)
        self.assertIn('pub struct ProviderPendingRootsPageV1', native)
        legacy_capture = ROOT / 'crates/oasis7/src/simulator/provider_material_capture.rs'
        host_capture = ROOT / 'crates/oasis7_runtime_host/src/agent_execution/provider_material_capture.rs'
        capture_sources = [p.read_text() for p in (legacy_capture, host_capture) if p.is_file()]
        self.assertTrue(capture_sources, 'provider_material_capture owner missing')
        for capture in capture_sources:
            self.assertNotIn('oasis7_world_service::provider_material', capture)
            self.assertNotIn('pub struct ProviderMaterialOwnerBindingV1', capture)
        self.assertTrue(
            any('oasis7_formal_protocol::provider_material' in capture for capture in capture_sources),
            'active provider_material_capture must consume oasis7_formal_protocol::provider_material',
        )
        store = (ROOT / 'crates/oasis7_viewer_runtime/src/viewer/runtime_live/provider_original_store.rs').read_text()
        codec = (ROOT / 'crates/oasis7_viewer_runtime/src/viewer/runtime_live/provider_original_store_codec.rs').read_text()
        self.assertIn('use oasis7_formal_protocol::provider_material::{', store)
        self.assertNotIn('use oasis7_world_service::provider_material::{', store)
        native_codec = (ROOT / 'crates/oasis7_world_service/src/provider_material_codec.rs').read_text()
        self.assertIn('use crate::provider_material::ProviderMaterialOwnerBindingV1;', native_codec)
        self.assertIn('pub fn encode_event(record: &EventRecord)', native_codec)
        self.assertIn('pub(super) use oasis7_world_service::provider_material_codec::*;', codec)

    def test_incomplete_composite_archive_host_cannot_pass_trusted_gate(self):
        """Original dependency delta cannot be unlocked by empty structs."""
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            owner = root / 'crates/oasis7_runtime_host/src/simulator_archive'
            facade = root / 'crates/oasis7/src/simulator'
            (owner / 'kernel_host').mkdir(parents=True)
            facade.mkdir(parents=True)
            (owner / 'mod.rs').write_text(
                'pub mod archive;\npub mod kernel_host;\npub mod persist;\n')
            for path, typ in (('kernel_host.rs', 'WorldKernel'),
                              ('persist.rs', 'WorldSnapshot'),
                              ('archive.rs', 'WorldEvent')):
                (owner / path).write_text(f'pub struct {typ} {{}}\n')
            for name in ('archive_origin.rs', 'codec.rs', 'forwarding.rs', 'journal_view.rs'):
                (owner / 'kernel_host' / name).write_text('')
            for name, target in (('archive_host_facade.rs', 'archive'),
                                 ('persist_host_facade.rs', 'persist'),
                                 ('kernel_host_host_facade.rs', 'kernel_host')):
                (facade / name).write_text(
                    f'pub use oasis7_runtime_host::simulator_archive::{target}::*;\n')
            (facade / 'mod.rs').write_text(
                '#[cfg(target_arch = "wasm32")]\n'
                'pub mod archive;\npub(crate) mod persist;\nmod kernel_host;\n'
                '// archive_host_facade.rs persist_host_facade.rs kernel_host_host_facade.rs\n')
            with self.assertRaises(AssertionError):
                require_materialized_native_archive(root)

    def test_viewer_wire_telemetry_uses_native_owner_without_replacing_host_archive(self):
        """Shipping Server/Client have one type identity; archive-bearing World types stay host-owned."""
        cli = (ROOT / 'crates/oasis7_client_cli/src/bin/oasis7_pure_api_client.rs').read_text()
        server = (ROOT / 'crates/oasis7_viewer_runtime/src/server.rs').read_text()
        test_source = (ROOT / 'crates/oasis7_client_cli/src/bin/oasis7_pure_api_client/tests.rs').read_text()
        self.assertIn('use oasis7_simulator::{AgentDecisionTrace, RunnerMetrics, WorldEventKind, WorldTime};', cli)
        self.assertLessEqual(
            {'PersistError', 'RunnerMetrics', 'WorldTime'}, flat_rust_imports(server, 'oasis7_simulator'),
        )
        require_materialized_native_archive(ROOT)
        self.require_viewer_archive_wire_owner(cli, server, test_source)
        self.assertNotIn('oasis7::simulator::AgentDecisionTrace', cli)
        self.assertNotIn('oasis7::simulator::RunnerMetrics', cli)
        self.assertIn('fn native_viewer_trace_metrics_preserve_original_wire_type_identity()', test_source)

    def require_viewer_archive_wire_owner(self, cli, server, test_source):
        """Accept only the intact old facade or the complete native owner transition."""
        old_cli = 'use oasis7::simulator::{WorldEvent, WorldSnapshot};'
        old_server = 'use oasis7::simulator::{RuntimePerfSnapshot, WorldEvent, WorldJournal, WorldSnapshot};'
        native_event = 'use oasis7_runtime_host::simulator_archive::archive::WorldEvent;'
        native_cli = 'use oasis7_runtime_host::simulator_archive::persist::WorldSnapshot;'
        native_server = 'use oasis7_runtime_host::simulator_archive::persist::{WorldJournal, WorldSnapshot};'
        kernel = 'use oasis7_runtime_host::simulator_archive::kernel_host::WorldKernel;'
        if old_cli in cli and old_server in server:
            self.assertNotIn(native_event, cli)
            self.assertNotIn(native_event, server)
            self.assertNotIn(native_cli, cli)
            self.assertNotIn(native_server, server)
            return
        self.assertNotIn(old_cli, cli)
        self.assertNotIn(old_server, server)
        for source in (cli, server):
            self.assertIn(native_event, source)
        self.assertIn(native_cli, cli)
        self.assertIn(native_server, server)
        self.assertTrue(
            any(
                'RuntimePerfSnapshot' in flat_rust_imports(server, owner)
                for owner in ('oasis7::simulator', 'oasis7_simulator', 'oasis7_runtime_host::simulator_archive')
            ),
            'server must import canonical RuntimePerfSnapshot owner',
        )
        self.assertNotIn('struct RuntimePerfSnapshot', server)
        self.assertIn(kernel, server)
        for module, name in (('persist', 'WorldSnapshot'), ('archive', 'WorldEvent')):
            self.assertIn(f'oasis7_runtime_host::simulator_archive::{module}::{name},', test_source)
        self.assertIn('oasis7_runtime_host::simulator_archive::kernel_host::WorldKernel::new()', test_source)

    def test_viewer_archive_transition_accepts_complete_native_owner(self):
        """The new native state must pass with actual source newlines, not escaped literals."""
        cli = (
            'use oasis7_runtime_host::simulator_archive::archive::WorldEvent;\n'
            'use oasis7_runtime_host::simulator_archive::persist::WorldSnapshot;'
        )
        server = (
            'use oasis7::simulator::RuntimePerfSnapshot;\n'
            'use oasis7_runtime_host::simulator_archive::archive::WorldEvent;\n'
            'use oasis7_runtime_host::simulator_archive::persist::{WorldJournal, WorldSnapshot};\n'
            'use oasis7_runtime_host::simulator_archive::kernel_host::WorldKernel;'
        )
        test_source = (
            'TypeId::of::<oasis7_runtime_host::simulator_archive::persist::WorldSnapshot>(),\n'
            '        TypeId::of::<oasis7::simulator::WorldSnapshot>(),\n'
            'TypeId::of::<oasis7_runtime_host::simulator_archive::archive::WorldEvent>(),\n'
            '        TypeId::of::<oasis7::simulator::WorldEvent>(),\n'
            'oasis7_runtime_host::simulator_archive::persist::WorldSnapshot,\n'
            'oasis7_runtime_host::simulator_archive::archive::WorldEvent,\n'
            'oasis7_runtime_host::simulator_archive::kernel_host::WorldKernel::new()'
        )
        self.require_viewer_archive_wire_owner(cli, server, test_source)
        grouped = server.replace(
            'use oasis7::simulator::RuntimePerfSnapshot;',
            'use oasis7_simulator::{\n    PersistError, RuntimePerfSnapshot, WorldTime,\n};',
        )
        self.require_viewer_archive_wire_owner(cli, grouped, test_source)
        for invalid in ('RuntimePerfSnapshot as Alias', 'ProviderSnapshot'):
            with self.subTest(invalid=invalid), self.assertRaisesRegex(AssertionError, 'canonical RuntimePerfSnapshot'):
                self.require_viewer_archive_wire_owner(cli, grouped.replace('RuntimePerfSnapshot', invalid), test_source)

    def test_viewer_archive_transition_rejects_partial_or_wrong_owner(self):
        cli = 'use oasis7::simulator::{WorldEvent, WorldSnapshot};'
        server = 'use oasis7::simulator::{RuntimePerfSnapshot, WorldEvent, WorldJournal, WorldSnapshot};'
        self.require_viewer_archive_wire_owner(cli, server, '')
        with self.assertRaises(AssertionError):
            self.require_viewer_archive_wire_owner(cli, '', '')
        with self.assertRaises(AssertionError):
            self.require_viewer_archive_wire_owner(
                cli.replace('oasis7::simulator', 'oasis7_simulator'),
                server.replace('oasis7::simulator', 'oasis7_simulator'),
                '',
            )
        with self.assertRaises(AssertionError):
            self.require_viewer_archive_wire_owner(
                'use oasis7_runtime_host::simulator_archive::archive::WorldEvent;',
                server, '',
            )

    def test_chain_composite_mirror_recovery_uses_native_l4_owner(self):
        """Shipping Chain mirror/legacy restore/BFT and original QA share one archive-bearing L4 type."""
        owner = ROOT / 'crates/oasis7_runtime_host/src/simulator_archive'
        self.assertIn('pub mod kernel_host;', (owner / 'mod.rs').read_text())
        self.assertIn('pub mod persist;', (owner / 'mod.rs').read_text())
        root = ROOT / 'crates/oasis7_chain_runtime/src/host'
        paths = (
            'execution_bridge/simulator_mirror.rs',
            'execution_bridge/driver.rs',
            'execution_bridge/driver_bft.rs',
            'execution_bridge/driver_bft_v2.rs',
            'execution_bridge/driver_startup_recovery.rs',
            'execution_bridge/tests/simulator_driver.rs',
            'execution_bridge/tests/driver.rs',
            'node_lifecycle/legacy_migration/verify.rs',
            'status_payload_world_resource.rs',
            'execution_bridge/tests/qa_conformance/viewer_process.rs',
        )
        for path in paths:
            source = (root / path).read_text()
            if path != 'execution_bridge/tests/simulator_driver.rs':
                self.assertNotIn('oasis7::simulator::WorldKernel', source, path)
                self.assertNotIn('oasis7::simulator::WorldSnapshot', source, path)
            self.assertNotIn('oasis7::simulator::WorldJournal', source, path)
        source = (root / 'execution_bridge/tests/simulator_driver.rs').read_text()
        self.assertIn('fn chain_native_composite_archive_has_one_type_identity_and_original_json_wire()', source)
        mirror = (root / 'execution_bridge/simulator_mirror.rs').read_text()
        self.assertIn('use oasis7_runtime_host::simulator_archive::kernel_host::WorldKernel;', mirror)

    def test_agent_memory_original_store_and_real_llm_consumers_share_native_l2_owner(self):
        """No legacy memory algorithm, shadow representation or host-owned duplicate."""
        native = (ROOT / 'crates/oasis7_simulator/src/agent_memory.rs').read_text()
        tests = (ROOT / 'crates/oasis7_simulator/src/agent_memory_tests.rs').read_text()
        self.assertIn('pub struct AgentMemory', native)
        self.assertIn('pub struct ShortTermMemory', native)
        self.assertIn('pub struct LongTermMemory', native)
        legacy_path = ROOT / 'crates/oasis7/src/simulator/memory.rs'
        if legacy_path.is_file():
            legacy = legacy_path.read_text()
            self.assertIn('pub use oasis7_simulator::{', legacy)
            self.assertNotIn('pub struct AgentMemory', legacy)
        legacy_llm = ROOT / 'crates/oasis7/src/simulator/llm_agent.rs'
        native_llm = ROOT / 'crates/oasis7_runtime_host/src/agent_execution/llm_agent.rs'
        llm_sources = [p.read_text() for p in (legacy_llm, native_llm) if p.is_file()]
        self.assertTrue(llm_sources, 'LlmAgentBehavior owner missing')
        for source in llm_sources:
            self.assertNotIn('pub struct AgentMemory', source)
            if 'pub struct LlmAgentBehavior' in source:
                self.assertIn('use oasis7_simulator::{AgentMemory', source)
        if legacy_llm.is_file() and 'pub struct LlmAgentBehavior' not in legacy_llm.read_text():
            self.assertIn('oasis7_runtime_host::', legacy_llm.read_text())
        legacy_selector = ROOT / 'crates/oasis7/src/simulator/llm_agent/memory_selector.rs'
        native_selector = ROOT / 'crates/oasis7_simulator/src/agent_memory_selector.rs'
        selector_sources = [p.read_text() for p in (legacy_selector, native_selector) if p.is_file()]
        self.assertTrue(selector_sources, 'memory_selector owner missing')
        for source in selector_sources:
            self.assertNotIn('pub struct AgentMemory', source)
        if legacy_selector.is_file() and 'fn collect_short_term_candidates(' in legacy_selector.read_text():
            self.assertIn(
                'use oasis7_simulator::{AgentMemory, LongTermMemoryEntry, MemoryEntry, MemoryEntryKind};',
                legacy_selector.read_text(),
            )
        else:
            self.assertTrue(native_selector.is_file())
            self.assertIn('AgentMemory', native_selector.read_text())
            if legacy_selector.is_file():
                self.assertIn('oasis7_simulator::agent_memory_selector', legacy_selector.read_text())
        self.assertIn('fn long_term_memory_export_restore_roundtrip_preserves_entries_and_next_id()', tests)
        self.assertIn('fn agent_memory_restores_long_term_entries()', tests)

    def test_chain_native_world_service_keeps_actual_gameplay_host_seam(self):
        """Fail closed for both accepted two-argument host seams during native migration.

        A trusted policy may migrate its producer before the consumers; neither
        the old nor native adapter may accidentally select the generic
        three-argument Service interface. Both real Chain consumers must use
        *one identical* adapter owner with matching executable evidence.
        """
        root = ROOT / 'crates/oasis7_chain_runtime/src/host'
        legacy_import = 'use oasis7::world_service::gameplay;'
        native_import = 'use oasis7_runtime_host::gameplay_service as gameplay;'
        accepted_owners = set()
        for relative in ('world_service_api.rs', 'execution_bridge/world_service_execution.rs'):
            source = (root / relative).read_text()
            self.assertIn('use oasis7_world_service::*;', source, relative)
            self.assertNotIn('use oasis7::world_service::*;', source, relative)
            self.assertNotIn('use oasis7_world_service::gameplay;', source, relative)
            self.assertIn('gameplay::authenticated_action', source, relative)
            legacy = legacy_import in source
            native = native_import in source
            self.assertNotEqual(legacy, native, relative)
            accepted_owners.add('legacy' if legacy else 'native')
        self.assertEqual(len(accepted_owners), 1, 'two Chain callers chose different gameplay owners')

        old_bridge = (ROOT / 'crates/oasis7/src/world_service/gameplay.rs').read_text()
        if 'legacy' in accepted_owners:
            self.assertIn('pub fn authenticated_action(', old_bridge)
            self.assertIn('pub fn authenticated_action_with_origin(', old_bridge)
            self.assertIn(
                'oasis7_runtime_host::gameplay_actions::build_runtime_action_from_gameplay_request',
                old_bridge,
            )
        else:
            native_path = ROOT / 'crates/oasis7_runtime_host/src/gameplay_service.rs'
            self.assertTrue(native_path.is_file())
            native_owner = native_path.read_text()
            self.assertIn('pub use oasis7_runtime_host::gameplay_service::*;', old_bridge)
            self.assertNotIn('pub fn authenticated_action(', old_bridge)
            self.assertIn('pub fn authenticated_action(', native_owner)
            self.assertIn('pub fn authenticated_action_with_origin(', native_owner)
            self.assertIn(
                'oasis7_world_service::gameplay::authenticated_action(',
                native_owner,
            )
            self.assertIn(
                'oasis7_world_service::gameplay::authenticated_action_with_origin(',
                native_owner,
            )
            self.assertIn(
                'crate::gameplay_actions::build_runtime_action_from_gameplay_request',
                native_owner,
            )
            self.assertNotIn('oasis7::', native_owner)
            self.assertNotIn('oasis7', manifest('oasis7_runtime_host')['dependencies'])
            nonce = (
                ROOT / 'crates/oasis7_chain_runtime/src/host'
                / 'execution_bridge/tests/qa_conformance/legacy_nonce.rs'
            ).read_text()
            self.assertIn(
                'oasis7_runtime_host::gameplay_service::authenticated_action',
                nonce,
            )

    def test_provider_tools_and_viewer_use_native_runtime_owner(self):
        """Reject old umbrella Runtime imports while preserving host Simulator seams."""
        native = manifest('oasis7_provider_tools')['dependencies']['oasis7_runtime']
        viewer = manifest('oasis7_viewer_runtime')['target']['cfg(not(target_arch = "wasm32"))']['dependencies']['oasis7_runtime']
        self.assertEqual(viewer, {'path': '../oasis7_runtime', 'default-features': False})
        self.assertEqual(native, {'path': '../oasis7_runtime', 'default-features': False})
        for relative in (
            'crates/oasis7_provider_tools/src/bin/oasis7_llm_agent_demo/runtime_bridge.rs',
            'crates/oasis7_provider_tools/src/bin/oasis7_llm_agent_demo/tests.rs',
            'crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs',
        ):
            source = (ROOT / relative).read_text()
            self.assertNotIn('oasis7::runtime::', source, relative)
            self.assertIn('oasis7_runtime::', source, relative)

    def test_chain_client_native_service_runtime_owner_contracts(self):
        """Keep the real Gameplay/Starter/Chat consumer paths on their canonical owners."""
        chain = ROOT / 'crates/oasis7_chain_runtime/src/host'
        client = ROOT / 'crates/oasis7_client_cli/src/bin/oasis7_pure_api_client'
        for relative in ('gameplay_submit_api.rs', 'execution_bridge/tests/world_service.rs'):
            source = (chain / relative).read_text()
            self.assertNotIn('oasis7::world_service::', source, relative)
            self.assertIn('oasis7_world_service::', source, relative)
        for relative in ('preview_world_init.rs', 'execution_bridge/tests/participant_ordinary_starter.rs'):
            source = (chain / relative).read_text()
            self.assertNotIn('oasis7::runtime_host::ordinary_starter::', source, relative)
            self.assertIn('oasis7_world_service::ordinary_starter::', source, relative)
        gameplay_test = (chain / 'execution_bridge/tests/world_service.rs').read_text()
        self.assertIn('use oasis7_runtime_host::gameplay_service as gameplay;', gameplay_test)
        self.assertNotIn('use oasis7_world_service::gameplay;', gameplay_test)
        self.assertIn('gameplay::authenticated_action_with_origin(&world,', gameplay_test)
        for relative in ('canonical_chat_fixture.rs', 'tests.rs'):
            source = (client / relative).read_text()
            self.assertNotIn('oasis7::runtime::', source, relative)
            self.assertIn('oasis7_runtime::', source, relative)
        self.assertEqual(
            manifest('oasis7_client_cli')['dependencies']['oasis7_runtime'],
            {'path': '../oasis7_runtime', 'default-features': False},
        )

    def test_native_composite_archive_original_wasm_and_replay_witnesses(self):
        """The real extracted L4 owner must retain original wasm and replay witnesses."""
        archive = ROOT / 'crates/oasis7_runtime_host/src/simulator_archive'
        original = ROOT / 'crates/oasis7/src/simulator'
        for filename, symbol in (
            ('archive.rs', 'pub struct WorldEvent {'),
            ('persist.rs', 'pub struct WorldSnapshot {'),
            ('kernel_host.rs', 'pub struct WorldKernel {'),
        ):
            self.assertIn(symbol, (archive / filename).read_text())
            self.assertIn(symbol, (original / filename).read_text())
        source = (archive / 'tests.rs').read_text()
        self.assertIn('original_composite_snapshot_extensions_survive_exact_native_split_rejoin', source)
        self.assertIn('native_composite_event_roundtrip_keeps_rejection_and_host_field', source)
        facade = (original / 'archive_host_facade.rs').read_text()
        self.assertIn('archive/tests.rs', facade)
        self.assertIn('archive/retention_tests.rs', facade)
        deps = manifest('oasis7_runtime_host')['dependencies']
        self.assertEqual(deps['oasis7_agent_harness'], {'path': '../oasis7_agent_harness'})
        self.assertEqual(deps['serde'], {'version': '1', 'features': ['derive']})
        self.assertEqual(deps['serde_json'], '1')

    def test_core_is_library_and_original_executable_names_survive_once(self):
        core = manifest('oasis7')
        self.assertIs(core['package']['autobins'], False)
        self.assertFalse(core.get('bin', []))
        self.assertFalse(list((ROOT / 'crates/oasis7/src/bin').glob('**/*.rs')))
        seen = set()
        for package, names in OWNERS.items():
            directory = ROOT / 'crates' / package
            data = manifest(package)
            declared = {item['name']: item for item in data.get('bin', [])}
            if (directory / 'src/main.rs').exists():
                declared.setdefault(package, {'path': 'src/main.rs'})
            if data['package'].get('autobins', True):
                for source in (directory / 'src/bin').glob('*.rs'):
                    declared.setdefault(source.stem, {'path': str(source.relative_to(directory))})
            self.assertEqual(set(declared), set(names), package)
            for name, target in declared.items():
                self.assertNotIn(name, seen)
                seen.add(name)
                self.assertTrue((directory / target['path']).is_file(), name)

    def test_newapi_service_has_no_transitive_world_dependency(self):
        visited = set()
        pending = ['oasis7_newapi_bridge_service']
        while pending:
            package = pending.pop()
            if package in visited:
                continue
            visited.add(package)
            data = manifest(package)
            tables = [data.get('dependencies', {})]
            tables += [target.get('dependencies', {}) for target in data.get('target', {}).values()]
            for table in tables:
                for name, dependency in table.items():
                    if isinstance(dependency, dict) and 'path' in dependency:
                        pending.append(dependency.get('package', name))
        self.assertNotIn('oasis7', visited)
        self.assertNotIn('oasis7_node', visited)
        self.assertNotIn('oasis7_wasm_executor', visited)

    def test_light_tool_and_provider_normal_build_dependency_boundaries(self):
        for root, forbidden in (
            ('oasis7_protocol_tools', {'oasis7', 'oasis7_runtime', 'oasis7_node', 'oasis7_wasm_executor', 'wasmtime'}),
            ('oasis7_simulator', {'oasis7', 'oasis7_ops_tools', 'oasis7_runtime', 'wasmtime'}),
            ('oasis7_model_provider', {'oasis7_agent_harness', 'oasis7_game_client', 'oasis7_client_api', 'oasis7'}),
            ('oasis7_agent_api', {'oasis7_agent_harness', 'oasis7_game_client', 'oasis7_model_provider', 'oasis7'}),
        ):
            pending, visited = [root], set()
            while pending:
                package = pending.pop()
                if package in visited:
                    continue
                visited.add(package)
                data = manifest(package)
                tables = [data.get(kind, {}) for kind in ('dependencies', 'build-dependencies')]
                for target in data.get('target', {}).values():
                    tables.extend(target.get(kind, {}) for kind in ('dependencies', 'build-dependencies'))
                for table in tables:
                    self.assertTrue(forbidden.isdisjoint(table), (root, package, forbidden.intersection(table)))
                    for name, dependency in table.items():
                        if isinstance(dependency, dict) and 'path' in dependency:
                            pending.append(dependency.get('package', name))
        tools = manifest('oasis7_protocol_tools')
        self.assertEqual(tools['features']['default'], [])
        self.assertNotIn('oasis7', tools['dev-dependencies'])
        self.assertEqual(tools['dev-dependencies']['oasis7_runtime'], {
            'path': '../oasis7_runtime',
            'default-features': False,
            'features': ['test_tier_required'],
        })
        self.assertEqual(tools['dev-dependencies']['oasis7_world_service'], {
            'path': '../oasis7_world_service',
            'default-features': False,
        })

    def test_client_production_does_not_enable_world_execution_features(self):
        data = manifest('oasis7_client_cli')
        self.assertEqual(data['features']['default'], [])
        if 'oasis7' in data['dependencies']:
            self.assertIs(data['dependencies']['oasis7']['default-features'], False)
            self.assertFalse(data['dependencies']['oasis7'].get('features', []))
        if 'oasis7_runtime' in data['dependencies']:
            self.assertIs(data['dependencies']['oasis7_runtime']['default-features'], False)
            self.assertNotIn('wasmtime', data['dependencies']['oasis7_runtime'].get('features', []))

    def test_storage_and_protocol_consumers_have_direct_owners(self):
        for package in ('oasis7_consensus', 'oasis7_wasm_materializer'):
            data = manifest(package)
            dependencies = dict(data.get('dependencies', {}))
            for target in data.get('target', {}).values():
                dependencies.update(target.get('dependencies', {}))
            self.assertIn('oasis7_cas', dependencies)
            self.assertNotIn('oasis7_distfs', dependencies)
        dependencies = manifest('oasis7_formal_protocol')['dependencies']
        self.assertTrue({'oasis7_distfs', 'oasis7_cas', 'zstd'}.isdisjoint(dependencies))

    def test_signer_protocol_and_client_storage_are_independent_foundations(self):
        for root, forbidden in (
            ('oasis7_signer_protocol', {'oasis7_local_signer', 'oasis7_context', 'oasis7_secure_envelope', 'libc', 'nix', 'argon2', 'chacha20poly1305'}),
            ('oasis7_client_storage', {'oasis7_game_client', 'oasis7', 'oasis7_distfs', 'oasis7_model_provider', 'reqwest'}),
        ):
            pending, visited = [root], set()
            while pending:
                package = pending.pop()
                if package in visited:
                    continue
                visited.add(package)
                data = manifest(package)
                tables = [data.get('dependencies', {})]
                tables += [target.get('dependencies', {}) for target in data.get('target', {}).values()]
                for table in tables:
                    self.assertTrue(forbidden.isdisjoint(table), (root, package))
                    for name, dependency in table.items():
                        if isinstance(dependency, dict) and 'path' in dependency:
                            pending.append(dependency.get('package', name))

    def test_client_proof_library_has_no_normal_world_or_node_dependency(self):
        pending, visited = ['oasis7_game_client'], set()
        while pending:
            package = pending.pop()
            if package in visited:
                continue
            visited.add(package)
            data = manifest(package)
            tables = [data.get('dependencies', {})]
            tables += [target.get('dependencies', {}) for target in data.get('target', {}).values()]
            for table in tables:
                for name, dependency in table.items():
                    if isinstance(dependency, dict) and 'path' in dependency:
                        pending.append(dependency.get('package', name))
        self.assertTrue({'oasis7', 'oasis7_node', 'oasis7_wasm_executor', 'oasis7_distfs', 'oasis7_cas'}.isdisjoint(visited))

    def test_service_correlation_payload_owner_is_native_and_legacy_source_is_retired(self):
        """Regression ratchet: canonical Service encoding must not move back into the umbrella."""
        old = ROOT / 'crates/oasis7/src/world_service/correlation.rs'
        current = ROOT / 'crates/oasis7_world_service/src/correlation.rs'
        self.assertFalse(old.exists(), old)
        source = current.read_text()
        for symbol in ('pub fn encode_consensus_intent(', 'pub fn encode_cleanup_consensus_v2('):
            self.assertIn(symbol, source)
        self.assertIn('crate::consensus_action_payload::*', source)
        legacy_mod = ROOT / 'crates/oasis7/src/world_service/mod.rs'
        if legacy_mod.is_file():
            self.assertNotIn('pub mod correlation;', legacy_mod.read_text())
        for package in ('oasis7_chain_runtime', 'oasis7_viewer_runtime'):
            for rust in (ROOT / 'crates' / package / 'src').rglob('*.rs'):
                self.assertNotIn('oasis7::world_service::correlation::', rust.read_text(), str(rust))

    def check_runtime_host_dependency_boundaries(self, load_manifest):
        """Enforce architectural invariants on oasis7_runtime_host without brittle exact-set snapshots."""
        required_core_edges = {
            'oasis7_runtime', 'oasis7_world_service', 'oasis7_formal_protocol', 'oasis7_agent_api',
            'oasis7_agent_harness', 'oasis7_client_api', 'oasis7_proto', 'oasis7_simulator',
            'oasis7_wasm_abi', 'serde', 'serde_json',
        }
        forbidden_transitive = {
            'oasis7',
            'oasis7_node', 'oasis7_net', 'oasis7_distfs', 'libp2p',
            'oasis7_chain_runtime', 'oasis7_viewer_runtime', 'oasis7_viewer', 'oasis7_viewer_codec',
            'oasis7_launcher_backend', 'oasis7_client_launcher', 'oasis7_launcher_ui',
            'oasis7_client_cli', 'oasis7_ops_tools', 'oasis7_protocol_tools', 'oasis7_provider_tools',
            'oasis7_newapi_bridge_service', 'oasis7_custody_worker', 'oasis7_dev_test_worker',
            'pixel_world_bridge',
        }
        forbidden_direct = forbidden_transitive | {'wasmtime'}

        host_spec = load_manifest('oasis7_runtime_host')
        host_deps = host_spec.get('dependencies', {})
        # Preserve the accepted URL-policy admission contract after replacing
        # the brittle dependency snapshot with architectural invariants.
        if 'url' in host_deps:
            policy_path = ROOT / 'crates/oasis7_runtime_host/src/provider_transport_url.rs'
            host_source = (ROOT / 'crates/oasis7_runtime_host/src/lib.rs').read_text()
            self.assertIn('pub mod provider_transport_url;', host_source)
            self.assertTrue(policy_path.is_file(), 'native provider URL owner missing')
            policy_source = policy_path.read_text()
            for required in (
                'pub fn validate_provider_http_base_url(',
                'url::Url;',
                'fn is_public_remote_https_host(',
                'loopback_accepts_only_http_and_explicit_loopback_hosts',
                'remote_requires_https_and_rejects_local_private_or_linklocal_hosts',
                'unknown_transport_and_url_syntax_fail_closed',
            ):
                self.assertIn(required, policy_source)
        self.assertTrue(
            required_core_edges.issubset(host_deps),
            f'missing core RuntimeHost dependencies: {sorted(required_core_edges - set(host_deps))}',
        )
        for core_pkg in ('oasis7_runtime', 'oasis7_world_service'):
            edge = host_deps[core_pkg]
            self.assertIsInstance(edge, dict, core_pkg)
            self.assertIs(edge.get('default-features'), False, core_pkg)
            self.assertNotIn('wasmtime', edge.get('features', ()), core_pkg)

        direct_tables = [host_spec.get(k, {}) for k in ('dependencies', 'build-dependencies', 'dev-dependencies')]
        for target in host_spec.get('target', {}).values():
            direct_tables.extend(target.get(k, {}) for k in ('dependencies', 'build-dependencies', 'dev-dependencies'))
        for table in direct_tables:
            resolved = {
                (dep.get('package', name) if isinstance(dep, dict) else name)
                for name, dep in table.items()
            } | set(table)
            self.assertTrue(
                forbidden_direct.isdisjoint(resolved),
                f'forbidden direct RuntimeHost dependency: {sorted(forbidden_direct & resolved)}',
            )

        pending, visited = ['oasis7_runtime_host'], set()
        while pending:
            package = pending.pop()
            if package in visited:
                continue
            visited.add(package)
            spec = load_manifest(package)
            tables = [spec.get(k, {}) for k in ('dependencies', 'build-dependencies')]
            for target in spec.get('target', {}).values():
                tables.extend(target.get(k, {}) for k in ('dependencies', 'build-dependencies'))
            for table in tables:
                resolved = {
                    (dep.get('package', name) if isinstance(dep, dict) else name)
                    for name, dep in table.items()
                } | set(table)
                self.assertTrue(
                    forbidden_transitive.isdisjoint(resolved),
                    (package, sorted(forbidden_transitive & resolved)),
                )
                for name, dep in table.items():
                    if isinstance(dep, dict) and 'path' in dep:
                        pending.append(dep.get('package', name))

    def test_runtime_host_dependency_boundaries_admit_legitimate_additions_and_reject_violations(self):
        import copy

        cache = {}

        def cached_manifest(pkg):
            if pkg not in cache:
                cache[pkg] = manifest(pkg)
            return copy.deepcopy(cache[pkg])

        self.check_runtime_host_dependency_boundaries(cached_manifest)

        # Legitimate additive host/provider/test dependencies must not require a prerequisite PR.
        def with_legitimate_additions(pkg):
            data = cached_manifest(pkg)
            if pkg == 'oasis7_runtime_host':
                data['dependencies']['url'] = '2'
                data.setdefault('target', {})['cfg(not(target_arch = "wasm32"))'] = {
                    'dependencies': {
                        'blake3': '1',
                        'hex': '0.4',
                        'oasis7_model_provider': {'path': '../oasis7_model_provider'},
                        'reqwest': {'version': '0.13.4', 'default-features': False, 'features': ['blocking', 'json', 'rustls']},
                        'toml': '0.9.12',
                        'tracing': '0.1',
                    }
                }
                data['dev-dependencies'] = {
                    'oasis7_hosted_access': {'path': '../oasis7_hosted_access', 'features': ['test-support']},
                    'oasis7_runtime': {'path': '../oasis7_runtime', 'default-features': False, 'features': ['test-support']},
                }
            return data

        self.check_runtime_host_dependency_boundaries(with_legitimate_additions)

        for label, mutate in (
            ('umbrella_direct', lambda d: d['dependencies'].__setitem__('oasis7', {'path': '../oasis7'})),
            ('umbrella_dev', lambda d: d.setdefault('dev-dependencies', {}).__setitem__('oasis7', {'path': '../oasis7'})),
            ('umbrella_aliased', lambda d: d['dependencies'].__setitem__('legacy_umbrella', {'package': 'oasis7', 'path': '../oasis7'})),
            ('surface_viewer', lambda d: d['dependencies'].__setitem__('oasis7_viewer_runtime', {'path': '../oasis7_viewer_runtime'})),
            ('surface_chain', lambda d: d['dependencies'].__setitem__('oasis7_chain_runtime', {'path': '../oasis7_chain_runtime'})),
            ('node_p2p', lambda d: d['dependencies'].__setitem__('oasis7_node', {'path': '../oasis7_node'})),
            ('direct_wasmtime', lambda d: d['dependencies'].__setitem__('wasmtime', '49')),
            ('runtime_default_features', lambda d: d['dependencies']['oasis7_runtime'].__setitem__('default-features', True)),
            ('runtime_wasmtime_feature', lambda d: d['dependencies']['oasis7_runtime'].__setitem__('features', ['wasmtime'])),
            ('missing_core_edge', lambda d: d['dependencies'].pop('oasis7_world_service')),
        ):
            with self.subTest(case=label):
                def mutated_manifest(pkg, fn=mutate):
                    data = cached_manifest(pkg)
                    if pkg == 'oasis7_runtime_host':
                        fn(data)
                    return data

                with self.assertRaises(AssertionError):
                    self.check_runtime_host_dependency_boundaries(mutated_manifest)

    def test_native_world_host_factory_has_one_owner_and_real_app_dependencies(self):
        native = ROOT / 'crates/oasis7_runtime_host/src/lib.rs'
        source = native.read_text()
        self.assertIn('pub fn install_runtime_capabilities(', source)
        self.assertIn('pub fn load_authoritative_recovery_generation(', source)
        self.assertIn('HostModuleSourceCompiler', source)
        umbrella_path = ROOT / 'crates/oasis7/src/runtime_host.rs'
        if umbrella_path.is_file():
            umbrella = umbrella_path.read_text()
            self.assertIn('pub use oasis7_runtime_host::{install_runtime_capabilities, world};', umbrella)
            self.assertNotIn('pub fn install_runtime_capabilities(', umbrella)
        self.check_runtime_host_dependency_boundaries(manifest)
        self.assertIn('pub mod simulator_archive;', source)
        require_materialized_native_archive(ROOT)
        for package in ('oasis7_chain_runtime', 'oasis7_viewer_runtime', 'oasis7_ops_tools', 'oasis7_provider_tools', 'oasis7_client_cli'):
            for rust in (ROOT / 'crates' / package / 'src').rglob('*.rs'):
                rust_source = rust.read_text()
                self.assertNotIn('oasis7::runtime_host::world::', rust_source, str(rust))
                self.assertNotIn('crate::runtime_host::world::', rust_source, str(rust))
                self.assertNotIn('oasis7::runtime_host::install_runtime_capabilities', rust_source, str(rust))
                self.assertNotIn('crate::runtime_host::install_runtime_capabilities', rust_source, str(rust))
        for package in ('oasis7_chain_runtime', 'oasis7_viewer_runtime', 'oasis7_ops_tools', 'oasis7_provider_tools', 'oasis7_client_cli'):
            spec = manifest(package)
            deps = dict(spec.get('dependencies', {}))
            for target in spec.get('target', {}).values():
                deps.update(target.get('dependencies', {}))
            self.assertIn('oasis7_runtime_host', deps, package)

        # The L4 bootstrap closure is enforced under this original, stable
        # architecture test identity; the trusted governance baseline disallows
        # silently introducing an unregistered standalone Python test.
        legacy = ROOT / 'crates/oasis7/src/runtime_host/bootstrap.rs'
        native = ROOT / 'crates/oasis7_runtime_host/src/bootstrap.rs'
        source = native.read_text()
        for symbol in (
            'pub fn bootstrap_formal_release_runtime_world(',
            'pub fn bootstrap_runtime_world_with_chunk_runtime(',
            'fn install_starter_factory_authorities(',
            'native_formal_release_bootstrap_installs_original_agent_and_resources',
        ):
            self.assertIn(symbol, source)
        self.assertNotIn('crate::runtime_host::', source)
        sidecar_fn = 'pub fn bootstrap_generated_sidecar_runtime_world_with_chunk_runtime('
        if legacy.is_file():
            legacy_source = legacy.read_text()
            self.assertIn('pub use oasis7_runtime_host::bootstrap::*;', legacy_source)
            self.assertNotIn('fn bootstrap_formal_release_runtime_world(', legacy_source)
            self.assertNotIn('fn install_starter_factory_authorities(', legacy_source)
            sidecar_owners = [text for text in (legacy_source, source) if sidecar_fn in text]
            self.assertEqual(len(sidecar_owners), 1, 'sidecar bootstrap must have exactly one owner')
            self.assertIn('WorldKernel::load_from_dir(', sidecar_owners[0])
        else:
            self.assertIn(sidecar_fn, source)
            self.assertIn('WorldKernel::load_from_dir(', source)
        for package in ('oasis7_chain_runtime', 'oasis7_viewer_runtime'):
            for rust in (ROOT / 'crates' / package / 'src').rglob('*.rs'):
                actual = rust.read_text()
                self.assertNotIn('oasis7::runtime_host::bootstrap::bootstrap_formal_release_runtime_world', actual, str(rust))
                self.assertNotIn('oasis7::runtime_host::bootstrap::bootstrap_runtime_world_with_chunk_runtime', actual, str(rust))
        host = manifest('oasis7_runtime_host')['dependencies']
        self.assertIn('oasis7_simulator', host)
        self.assertNotIn('oasis7', host)

    def test_world_library_does_not_own_launcher_or_network_hosting(self):
        source = ROOT / 'crates/oasis7/src'
        self.assertFalse((source / 'launcher_bootstrap_peers.rs').exists())
        for name in ('server.rs', 'web_bridge.rs', 'game_read_api.rs'):
            self.assertFalse((source / 'viewer' / name).exists())
            self.assertTrue((ROOT / 'crates/oasis7_viewer_runtime/src' / name).is_file())
        core = manifest('oasis7')
        native = core['target']['cfg(not(target_arch = "wasm32"))']['dependencies']
        self.assertTrue({'ctrlc', 'tracing-subscriber', 'tungstenite', 'async-openai'}.isdisjoint(native))
        self.assertNotIn('multiaddr', core['dependencies'])

    def test_shared_launch_configuration_and_artifact_adapter_do_not_depend_on_world(self):
        for package in ('oasis7_launcher_config', 'oasis7_wasm_materializer'):
            data = manifest(package)
            tables = [data.get('dependencies', {})]
            tables += [target.get('dependencies', {}) for target in data.get('target', {}).values()]
            for table in tables:
                self.assertTrue({'oasis7', 'oasis7_node'}.isdisjoint(table), package)

    def test_no_cross_package_source_borrowing_from_binary_directories(self):
        for package in ['oasis7', *OWNERS, 'oasis7_hosted_access', 'oasis7_letai_credit_adapter']:
            directory = ROOT / 'crates' / package
            for source in (directory / 'src').rglob('*.rs'):
                for relative in re.findall(r'#\[path\s*=\s*"([^"]+)"\]', source.read_text()):
                    target = (source.parent / relative).resolve()
                    if '/src/bin/' in str(target):
                        self.assertTrue(target.is_relative_to(directory), f'{source}: {relative}')


if __name__ == '__main__':
    unittest.main()
