import { render } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { expect, it } from 'vitest';
import { PixelWorldCanvasRenderer } from './pixel_world_host.jsx';

it('keeps pending and unavailable feedback inside the responsive selection HUD, without claiming selection',()=>{
  const [status,setStatus]=createSignal('pending');
  const view=render(()=><PixelWorldCanvasRenderer locale={()=> 'en'} renderState={()=>null}
    selection={()=>null} selectionStatus={status} hoveredEntity={()=>null}
    visualOverlayEnabled={()=>false} onHover={()=>{}} rendererStatus={()=> 'ready'} />);
  try {
    const feedback=view.container.querySelector('[data-world-selection-status]');
    expect(feedback.closest('.pixel-world-canvas__selection')).not.toBeNull();
    expect(feedback.closest('.pixel-world-canvas')).not.toBeNull();
    expect(feedback.textContent).toContain('Checking');
    expect(view.container.querySelector('[data-selected=true]')).toBeNull();
    setStatus('unavailable');
    expect(feedback.textContent).toContain('could not be confirmed');
    setStatus(null);
    expect(view.container.querySelector('[data-world-selection-status]')).toBeNull();
  } finally {view.unmount();}
});

it('publishes correlation attributes without turning a new token into a new visible cue',()=>{
 const [feedback,setFeedback]=createSignal({intent_generation:1,ref:{kind:'agent',id:'B'},world_id:'W',scope_generation:1,phase:'pending'});
 const [status,setStatus]=createSignal('pending');
 const view=render(()=><PixelWorldCanvasRenderer locale={()=> 'en'} renderState={()=>null}
 selection={()=>null} selectionStatus={status} selectionFeedback={feedback} hoveredEntity={()=>null}
 visualOverlayEnabled={()=>false} onHover={()=>{}} rendererStatus={()=> 'ready'} />);
 try {
  const chip=view.container.querySelector('[data-world-feedback-intent]'),text=chip.textContent;
  expect(chip.dataset).toMatchObject({worldFeedbackIntent:'1',worldFeedbackKind:'agent',worldFeedbackId:'B',worldFeedbackWorld:'W',worldFeedbackScope:'1',worldFeedbackPhase:'pending'});
  setFeedback({...feedback(),intent_generation:2});expect(chip.dataset.worldFeedbackIntent).toBe('2');
  expect(chip.textContent).toBe(text);expect(view.container.querySelector('[data-selected=true]')).toBeNull();
  setFeedback(null);expect(chip.hasAttribute('data-world-feedback-intent')).toBe(false);
  setStatus(null);expect(view.container.querySelector('.pixel-world-canvas__selection')).toBeNull();
 } finally {view.unmount();}
});
