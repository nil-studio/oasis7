# oasis7: CI 测试分级细化到 Test Case设计

- 对应需求文档：[专题 PRD](ci-testcase-tiering.prd.md#case-tier-requirements)
- 本文件仅为兼容入口，不维护独立设计正文或运行结论。

<a id="1-设计定位"></a>
<a id="2-设计结构"></a>
<a id="3-关键接口--入口"></a>
<a id="4-约束与边界"></a>
<a id="5-设计演进计划"></a>
<a id="compatibility-route"></a>
<a id="当前去向与保留边界"></a>
## 当前去向与保留边界：兼容入口范围

设计正文、失败边界与验证责任已由 [CI 分级执行共享设计](ci-tiered-execution.design.md#ci-case-tier-design) 承接，请直接阅读该节。

### 保留决策与风险

本文不要求重复实现或增加审批。旧 URL 暂留，核清全部引用、配对、动态/外部分发消费者与 corpus 后才可删除。
[合并前原文](https://github.com/eng-cc/oasis7/blob/2adfc08706f1905c131adc0597a9ae5e99ceb2dd/doc/testing/ci/ci-testcase-tiering.design.md)保留历史来源，不作为当前操作或验收依据。

### 2.1 需求承接与分配表

| 上游要求 | 分配内容 | 本设计条款 | 主责 | 限制 |
| --- | --- | --- | --- | --- |
| [专题要求](ci-testcase-tiering.prd.md#case-tier-requirements) | 通过唯一接收章节定位完整设计 | [兼容路由](#compatibility-route) | 接收文档原 owner | 本文件不是第二份机制设计；产品适用性：N/A；reason=本行只维护旧设计 URL 到接收正文的导航，不承接或重定义其产品功能；scope=本文件兼容路由、旧锚点与接收条款可达性。接收正文的产品及专业要求与独立验证责任不变 |

### 11.1 验证映射表

| 上游要求 | 本设计条款 | 验收内容 | 验证来源 | 记录 | 限制 |
| --- | --- | --- | --- | --- | --- |
| [专题要求](ci-testcase-tiering.prd.md#case-tier-requirements) | [兼容路由](#compatibility-route) | 同候选链接到存在的接收节；接收节保有原职责与独立验证 | [文档检查](../../../scripts/doc-governance-check.test.sh)及接收节逐项映射审读 | 修改 diff、链接核对及实际检查输出 | 文档检查不证明领域运行或发布通过 |
