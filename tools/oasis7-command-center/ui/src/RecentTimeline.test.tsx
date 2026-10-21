import {cleanup,fireEvent,render,screen} from '@solidjs/testing-library';
import {afterEach,expect,it,vi} from 'vitest';
import RecentTimeline from './RecentTimeline';
import type {WorkItem} from './generated/core';
afterEach(cleanup);
it('groups actual dates while preserving event order, exact timestamps and navigation',()=>{
 const works=['2026-10-20T12:00:00Z','2026-10-20T11:00:00Z','2026-10-18T12:00:00Z'].map((eventAt,index)=>({purpose:{value:`成果 ${index}`},result:{value:'PR MERGED',evidence:{eventAt}}} as WorkItem));
 const open=vi.fn();const {container}=render(()=><RecentTimeline works={works} open={open} status={()=>'PR 已合入'}/>);
 expect(screen.getAllByRole('heading')).toHaveLength(2);expect(container.querySelectorAll('li')).toHaveLength(3);
 expect(screen.getAllByRole('button').map(button=>button.textContent)).toEqual(['成果 0','成果 1','成果 2']);
 const exact=screen.getByText('2026-10-20T12:00:00Z');expect(exact).not.toBeVisible();fireEvent.click(exact.closest('details')!.querySelector('summary')!);expect(exact).toBeVisible();
 fireEvent.click(screen.getByRole('button',{name:'成果 0'}));expect(open).toHaveBeenCalledWith(works[0],expect.any(HTMLElement));
});
