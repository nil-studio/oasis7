import { render, fireEvent } from '@solidjs/testing-library';
import { describe, expect, it, vi } from 'vitest';
import { createSignal } from 'solid-js';
import { PixelWorldRendererTargets, moduleTargetOffsets, rendererEntityTargetStyle } from './pixel_world_renderer_targets.jsx';

describe('real renderer accessible projection', () => {
  it('keeps partially visible hit boxes at each edge until their full bounds leave the canvas', () => {
    for (const [x, y, visible] of [[-1,50,true],[101,50,true],[50,-1,true],[50,101,true],[-22,50,false],[122,50,false],[50,-22,false],[50,122,false]]) {
      const style = rendererEntityTargetStyle({id:'edge',pos:{x_cm:50,y_cm:50}}, {width_cm:100,depth_cm:100}, {width:100,height:100}, {pan_x_px:x-50,pan_y_px:y-50});
      expect(style.display, `${x},${y}`).toBe(visible ? undefined : 'none');
    }
  });
  it('preserves fractional camera pan and double-precision snap thresholds', () => {
    const size={width:1280,height:720};
    const center=rendererEntityTargetStyle({id:'A',pos:{x_cm:50,y_cm:50}}, {width_cm:100,depth_cm:100},size,{pan_x_px:0.6});
    expect(parseFloat(center.left)*1280/100).toBe(640);
    for (const [delta, expected] of [[-1e-8,640],[1e-8,642]]) {
      const position={x_cm:(641+delta-20)/1240*100,y_cm:50};
      const style=rendererEntityTargetStyle({id:'A',pos:position},{width_cm:100,depth_cm:100},size,{});
      expect(parseFloat(style.left)*1280/100).toBeCloseTo(expected,8);
    }
  });
  it('keeps fractional CSS dimensions in the same snap cell as Rust', () => {
    const width=1365.3333333333;
    const style=rendererEntityTargetStyle({id:'A',pos:{x_cm:1728326,y_cm:50}},{width_cm:10000000,depth_cm:100},{width,height:768},{},null,{width:1365,height:768});
    expect(parseFloat(style.left)*width/100).toBeCloseTo(250.061050061044,8);
  });
  const bounds = { width_cm: 1000, depth_cm: 1000 };
  const agent = { id: 'agent-0', pos: { x_cm: 250, y_cm: 600 } };
  it('retains focused targets across snapshots while updating data, geometry and membership', async () => {
    const other = { id: 'agent-1', pos: { x_cm: 100, y_cm: 200 } };
    const location = { id: agent.id, label: 'Depot', pos: { x_cm: 300, y_cm: 400 } };
    const snapshot = () => ({ world_bounds: bounds, agents: [{...agent}, {...other}], locations: [{...location}] });
    const [state, setState] = createSignal(snapshot());
    const [selection, setSelection] = createSignal(null);
    const select = vi.fn();
    const hover = vi.fn();
    const size = { width: 960, height: 540 };
    const view = render(() => <PixelWorldRendererTargets locale={() => 'en'} renderState={state} stageSize={() => size} selection={selection} onSelect={select} onHover={hover} />);
    const target = view.container.querySelector('[data-agent-id="agent-0"]');
    const depot = view.getByRole('button', {name: 'Select Depot'});
    target.focus();
    setState(snapshot());
    expect(view.container.querySelector('[data-agent-id="agent-0"]')).toBe(target);
    expect(document.activeElement).toBe(target);
    const moved = {...agent, label: 'Courier', pos: { x_cm: 700, y_cm: 300 }};
    setState({world_bounds: bounds, agents: [moved, {...other}], locations: [{...location, label: 'Warehouse'}]});
    expect(target).toHaveAccessibleName('Select Courier');
    expect(target.style.left).toBe(rendererEntityTargetStyle(moved, bounds, size).left);
    expect(document.activeElement).toBe(target);
    expect(view.getByRole('button', {name: 'Select Warehouse'})).toBe(depot);
    setSelection({kind: 'agent', id: agent.id});
    expect(target).toHaveAttribute('aria-pressed', 'true');
    expect(depot).toHaveAttribute('aria-pressed', 'false');
    setState({world_bounds: bounds, agents: [{...other}, {...moved}], locations: [{...location}]});
    expect([...view.container.querySelectorAll('[data-agent-id]')].map(node => node.dataset.agentId)).toEqual(['agent-1', 'agent-0']);
    expect(view.container.querySelector('[data-agent-id="agent-0"]')).toBe(target);
    await fireEvent.click(target);
    await fireEvent.mouseEnter(target);
    expect(select).toHaveBeenCalledWith({kind: 'agent', id: agent.id});
    expect(hover).toHaveBeenCalledWith({kind: 'agent', id: agent.id});
    setState({world_bounds: bounds, agents: [{...moved}], locations: []});
    expect(view.getByRole('button')).toBe(target);
    expect(depot.isConnected).toBe(false);
    setState({world_bounds: bounds, agents: [], locations: []});
    expect(target.isConnected).toBe(false);
    expect(view.queryByRole('button')).toBeNull();
  });
  it('normalizes Chinese locale aliases and does not synthesize absent renderer locations', () => {
    const view = render(() => <PixelWorldRendererTargets locale={() => 'zh-CN'} renderState={() => ({ agents: [{id:'agent-0'}], locations:[{id:'location-0',pos:{x_cm:1,y_cm:1}}] })} stageSize={() => ({width:960,height:540})} selection={() => null} onSelect={() => {}} onHover={() => {}} />);
    expect(view.getByRole('button')).toHaveAccessibleName('选择 行动体 0');
    expect(view.container.querySelector('[data-location-id]')).toBeNull();
  });
  it('centers the hit area over the camera-focused GPU agent rather than the old world-percent marker', () => {
    const style = rendererEntityTargetStyle(agent, bounds, { width: 1440, height: 900 }, { zoom: 1, pan_x_px: 350, pan_y_px: -86 });
    expect(style.left).toBe('50%');
    expect(style.top).toBe('50%');
    expect(style.transform).toBe('translate(-50%, -50%)');
    expect(style.width).toBe('44px');
  });
  it('updates pan, zoom and resize without changing selection identity', async () => {
    const [camera, setCamera] = createSignal({ zoom: 1, pan_x_px: 0, pan_y_px: 0 });
    const [size, setSize] = createSignal({ width: 1440, height: 900 });
    const select = vi.fn();
    const view = render(() => <PixelWorldRendererTargets locale={() => 'en'} renderState={() => ({ world_bounds: bounds, agents: [agent], locations: [] })} cameraState={camera} stageSize={size} selection={() => ({kind:'agent',id:'agent-0'})} onSelect={select} onHover={() => {}} />);
    const target = view.getByRole('button');
    const initial = target.style.left;
    setCamera({ zoom: 2, pan_x_px: 200, pan_y_px: -86 });
    expect(target.style.left).not.toBe(initial);
    setSize({ width: 390, height: 844 });
    expect(target.style.left).toBe(rendererEntityTargetStyle(agent,bounds,size(),camera()).left);
    await fireEvent.click(target);
    expect(select).toHaveBeenCalledWith({kind:'agent',id:'agent-0'});
    expect(view.container.querySelectorAll('[data-agent-id="agent-0"]')).toHaveLength(1);
  });

  it('reprojects targets when a late backing-size update arrives after mount', () => {
    const cssSize = { width: 1440, height: 1000 };
    const camera = { zoom: 1.563, pan_x_px: 932, pan_y_px: -523 };
    const [backingSize, setBackingSize] = createSignal({ width: 960, height: 540 });
    const view = render(() => <PixelWorldRendererTargets
      locale={() => 'en'}
      renderState={() => ({ world_bounds: { width_cm: 10_000_000, depth_cm: 5_000_000 }, agents: [{ id: 'agent-0', pos: { x_cm: 2_900_000, y_cm: 3_450_000, z_cm: 0 } }], locations: [] })}
      cameraState={() => camera}
      stageSize={() => cssSize}
      rendererSize={backingSize}
      selection={() => null}
      onSelect={() => {}}
      onHover={() => {}}
    />);
    const target = view.container.querySelector('[data-renderer-target="true"]');
    const initial = target.style.left;
    setBackingSize({ width: 2880, height: 2000 });
    expect(target.style.left).not.toBe(initial);
    expect(target.style.left).toBe(rendererEntityTargetStyle(
      { id: 'agent-0', pos: { x_cm: 2_900_000, y_cm: 3_450_000, z_cm: 0 } },
      { width_cm: 10_000_000, depth_cm: 5_000_000 },
      cssSize,
      camera,
      undefined,
      { width: 2880, height: 2000 },
    ).left);
  });

  it('projects in CSS units and snaps through the actual framebuffer at DPR2', () => {
    const cssSize = { width: 1440, height: 900 };
    const backingSize = { width: 2880, height: 1800 };
    const camera = { zoom: 1.563, pan_x_px: 932, pan_y_px: -523 };
    const renderBounds = { width_cm: 10_000_000, depth_cm: 5_000_000 };
    const agent = { id: 'agent-0', pos: { x_cm: 2_900_000, y_cm: 3_450_000, z_cm: 0 } };
    const location = { id: 'loc-0', label: 'Factory', pos: { x_cm: 4_300_000, y_cm: 3_100_000, z_cm: 0 } };
    const moduleAgent = { id: 'module-agent', kind: 'beacon', pos: agent.pos };
    const moduleLocation = { id: 'module-location', kind: 'relay', pos: location.pos };
    const center = (style) => ({
      x: parseFloat(style.left) * cssSize.width / 100,
      y: parseFloat(style.top) * cssSize.height / 100,
    });

    // F=2880x1800: s=4, each logical pixel is 2 CSS px; pan y=-523 snaps to -522.
    expect(center(rendererEntityTargetStyle(agent, renderBounds, cssSize, camera, null, backingSize))).toEqual({
      x: expect.closeTo(1192, 4),
      y: expect.closeTo(184, 4),
    });
    expect(center(rendererEntityTargetStyle(location, renderBounds, cssSize, camera, null, backingSize))).toEqual({
      x: expect.closeTo(1498, 4),
      y: expect.closeTo(90, 4),
    });
    expect(center(rendererEntityTargetStyle(moduleAgent, renderBounds, cssSize, camera, { x: -48, y: -48 }, backingSize))).toEqual({
      x: expect.closeTo(1144, 4),
      y: expect.closeTo(136, 4),
    });
    expect(center(rendererEntityTargetStyle(moduleLocation, renderBounds, cssSize, camera, { x: -48, y: -48 }, backingSize))).toEqual({
      x: expect.closeTo(1450, 4),
      y: expect.closeTo(42, 4),
    });
    expect(1192 - 1144).toBeCloseTo(48, 4);
    expect(184 - 136).toBeCloseTo(48, 4);
  });

  it('keeps co-anchored parent and module DOM targets aligned and independently clickable', async () => {
    const anchor = { x_cm: 250, y_cm: 600 };
    const locationAnchor = { x_cm: 100, y_cm: 200 };
    const select = vi.fn();
    const size = { width: 960, height: 540 };
    const view = render(() => <PixelWorldRendererTargets
      locale={() => 'en'}
      renderState={() => ({
        world_bounds: bounds,
        agents: [{ id: 'agent-0', pos: anchor }],
        locations: [{ id: 'loc-0', label: 'Depot', pos: locationAnchor }],
        module_visual_entities: [
          { id: 'module-agent-a', kind: 'beacon', pos: anchor },
          { id: 'module-agent-b', kind: 'relay', pos: anchor },
          { id: 'module-location-a', kind: 'artifact', pos: locationAnchor },
        ],
      })}
      stageSize={() => size}
      selection={() => null}
      onSelect={select}
      onHover={() => {}}
    />);
    const center = (target) => ({
      x: parseFloat(target.style.left) * size.width / 100,
      y: parseFloat(target.style.top) * size.height / 100,
    });
    const agent = view.container.querySelector('[data-agent-id="agent-0"]');
    const location = view.container.querySelector('[data-location-id="loc-0"]');
    const agentModuleA = view.container.querySelector('[data-module-id="module-agent-a"]');
    const agentModuleB = view.container.querySelector('[data-module-id="module-agent-b"]');
    const locationModule = view.container.querySelector('[data-module-id="module-location-a"]');
    expect(center(agentModuleA)).toEqual({ x: center(agent).x - 48, y: center(agent).y - 48 });
    expect(center(agentModuleB)).toEqual({ x: center(agent).x, y: center(agent).y - 48 });
    expect(center(locationModule)).toEqual({ x: center(location).x - 48, y: center(location).y - 48 });
    expect(Math.abs(center(agentModuleA).x - center(agent).x)).toBeGreaterThanOrEqual(44);
    expect(Math.abs(center(agentModuleA).y - center(agent).y)).toBeGreaterThanOrEqual(44);

    await fireEvent.click(agent);
    await fireEvent.click(location);
    await fireEvent.click(agentModuleA);
    await fireEvent.click(agentModuleB);
    await fireEvent.click(locationModule);
    expect(select.mock.calls).toEqual([
      [{ kind: 'agent', id: 'agent-0' }],
      [{ kind: 'location', id: 'loc-0' }],
      [{ kind: 'module_visual', id: 'module-agent-a' }],
      [{ kind: 'module_visual', id: 'module-agent-b' }],
      [{ kind: 'module_visual', id: 'module-location-a' }],
    ]);
  });

  it('keeps deterministic co-anchor slots at the 4096 module capacity', () => {
    const anchor = { x_cm: 250, y_cm: 600, z_cm: 0 };
    const modules = Array.from({ length: 4096 }, (_, index) => ({
      id: `module-${String(index).padStart(4, '0')}`,
      kind: 'relay',
      pos: anchor,
    })).reverse();
    const offsets = moduleTargetOffsets({ agents: [], locations: [], moduleVisualEntities: modules });

    expect(offsets.size).toBe(4096);
    expect(offsets.get('module-0000')).toEqual({ x: -48, y: -48 });
    expect(offsets.get('module-0001')).toEqual({ x: 0, y: -48 });
    expect(offsets.get('module-0008')).toEqual({ x: -96, y: -96 });
    expect(offsets.get('module-4095')).toEqual({ x: 24576, y: 24576 });
  });
  it('keeps co-anchor native hit boxes apart at six CSS pixels per logical pixel', () => {
    const modules = [{id: 'a', kind: 'artifact', pos: {x_cm: 1,y_cm: 1}}, {id: 'b', kind: 'artifact', pos: {x_cm: 1,y_cm: 1}}];
    const offsets = moduleTargetOffsets({agents: [], locations: [], moduleVisualEntities: modules}, {width:3840,height:2160}, {width:3840,height:2160});
    expect(offsets.get('a')).toEqual({x:-102,y:-102});
    expect(offsets.get('b')).toEqual({x:0,y:-102});
  });

});
