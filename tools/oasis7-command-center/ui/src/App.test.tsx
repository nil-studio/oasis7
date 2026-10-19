import { fireEvent, render, screen, cleanup } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import App from './App';
import { acceptsEnter, bridge, statusLabel } from './bridge';
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => false, invoke: vi.fn() }));
afterEach(()=>{cleanup();localStorage.clear();});
describe('explicit unavailable-source boundary', () => {
  it('does not invent project facts or completion for browser preview', () => {
    render(() => <App />);
    expect(screen.getByText(/浏览器预览未接入桌面 IPC/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '打开项目' })).toBeDisabled();
    expect(screen.getByLabelText('文字与界面大小')).toHaveValue('1');
    expect(document.body.textContent).not.toMatch(/完成率\s*\d+%/);
    fireEvent.click(screen.getByRole('button', {name: /待我处理/}));
    expect(screen.getByText(/无法核实是否有等待输入的工作/)).toBeInTheDocument();
  });
  it('never forwards commands in browser preview', async () => {
    await expect(bridge.openProject('/tmp/project')).rejects.toThrow('未接入桌面核心');
  });
});
describe('Chinese composition and source states', () => {
  it('rejects candidate Enter from all observed composition forms', () => {
    expect(acceptsEnter({key:'Enter',isComposing:true},false)).toBe(false);
    expect(acceptsEnter({key:'Enter',isComposing:false},true)).toBe(false);
    expect(acceptsEnter({key:'Enter',isComposing:false,keyCode:229},false)).toBe(false);
    expect(acceptsEnter({key:'Enter',isComposing:false},false)).toBe(true);
  });
  it('retains distinct empty, partial, failed, stale and unavailable labels', () => {
    expect(new Set(['empty','partial','error','stale','unconnected'].map(statusLabel)).size).toBe(5);
  });
  it('keeps unsaved decision text when switching views and searching', () => {
    render(() => <App />);
    fireEvent.click(screen.getByRole('button',{name:/规范与决策/}));
    fireEvent.input(screen.getByLabelText('决定内容'),{target:{value:'保留中文草稿'}});
    fireEvent.click(screen.getByRole('button',{name:/开发执行/}));
    fireEvent.click(screen.getByRole('button',{name:/规范与决策/}));
    expect(screen.getByLabelText('决定内容')).toHaveValue('保留中文草稿');
  });
});

it('applies and persists display size without reading sources or sending work',()=>{
 const read=vi.spyOn(bridge,'snapshot');render(()=><App/>);
 fireEvent.change(screen.getByLabelText('文字与界面大小'),{target:{value:'2'}});
 expect(screen.getByLabelText('文字与界面大小')).toHaveValue('2');
 expect(localStorage.getItem('oasis7.display-scale')).toBe('2');expect(read).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'重置'}));expect(screen.getByLabelText('文字与界面大小')).toHaveValue('1');read.mockRestore();
});
