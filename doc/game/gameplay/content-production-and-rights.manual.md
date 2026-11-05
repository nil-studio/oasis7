# 游戏内容来源、制作交接与成本核对手册

<a id="scope"></a>
## 1. 适用范围与结果归属

本手册验证[制作范围、来源与成本设计](content-production-and-rights.design.md)的具体输入、方法与边界。适用于内容采用、来源替换、生产批次或阶段结论；纯文档修改可以执行静态盘点、关系审读与算例核算，不必为此运行游戏、重新制作素材或采购服务。

执行前固定所检查的 source revision、所选内容／用途、适用平台和语言；需要运行结论时另固定真实构建／资源／环境。结果记入原 Task／PR、制作入口和既有构建／QA 证据，不新建全仓资产状态表。每项分别说明观察到的文件／事实、已执行的方法、未取得的材料和结论边界。

## 2. 具体检查方法

<a id="cp-scope"></a>
### CP-SCOPE：重算范围并核对单位

从仓库根执行下列只读脚本。`source` 使用设计审读基线；评估另一候选时显式替换为已确认的 commit，输出中的实际 source 必须与引用范围一致。脚本只列 Git 受跟踪集合，不扫描本机缓存、不生成新的资产索引。

```bash
python3 - <<'PY'
import json
import subprocess
from collections import Counter
from pathlib import PurePosixPath

source = 'f4f8e1bb1be6cb9746ea767af3bdf71204e347e3'
paths = subprocess.check_output(
    ['git', 'ls-tree', '-r', '--name-only', source], text=True
).splitlines()

def selected(prefix, suffixes):
    return [p for p in paths if p.startswith(prefix)
            and PurePosixPath(p).suffix.lower() in suffixes]

groups = {
    'scenario_json': selected('crates/oasis7_simulator/scenarios/', {'.json'}),
    'viewer_reference_svg': selected('doc/world-simulator/viewer/assets/ui-ux/', {'.svg'}),
    'site_image': selected('site/assets/images/', {'.svg', '.png', '.webp'}),
    'social_html': selected('site/social/', {'.html'}),
    'social_png': selected('site/social/', {'.png'}),
    'v50_reading_unit': [p for p in paths
        if p.startswith('site/story/releases/v50/')
        and p.endswith('.md') and not p.endswith('/README.md')],
}
modules = [p for p in paths
    if p.startswith('crates/oasis7_builtin_wasm_modules/')
    and p.count('/') == 3 and p.endswith('/Cargo.toml')]
catalog_path = 'doc/world-simulator/viewer/assets/ui-ux/full-function/coverage-catalog.json'
catalog = json.loads(subprocess.check_output(
    ['git', 'show', source + ':' + catalog_path], text=True
))
print('source', source)
for name, members in groups.items():
    print(name, len(members), dict(Counter(PurePosixPath(p).suffix for p in members)))
print('builtin_source_packages', len(modules))
print('catalog_features', len(catalog['features']), 'catalog_source', catalog['source_revision'])
print('scenario_members', [PurePosixPath(p).name for p in groups['scenario_json']])
print('module_members', [p.split('/')[2] for p in modules])
print('site_image_members', groups['site_image'])
PY
```

在指定基线，预期得到场景 12、Viewer SVG 173、Site 图像 23（SVG 7／WebP 13／PNG 3）、social HTML 20／PNG 66、V50 阅读单元 17、builtin 源包 38、catalog 条目 107。数量不一致时先核 source 与集合定义，不修改原文件来迎合旧数字。

随后做语义检查，机械计数不能替代下列步骤：

1. 逐项对照[首局 C01～C09](../../world-simulator/viewer/first-session-content-delivery.design.md#content-units)、[native 目录](industrial-native-content-delivery.design.md#native-content-catalog)、[四类视觉内容](../../world-simulator/viewer/viewer-world-visual-content-delivery.design.md#visual-content-catalog)；确认“单元”“ID”“分支／回退”与文件不同。
2. 从 38 个包中单列 local_test_provider；确认 M1 10、M4 factory 3／product 8／recipe 11、M5 5。核 Miner 不在 native 2-kind 目录内，不能将目录覆盖反向推广到全部包。
3. 将 12 个场景与[原完整场景表](../../world-simulator/scenario/scenario-files.prd.md)对应；保留文件场景与程序构造 formal seed 的差异。不能把四个碎片专题的细化范围写成全部场景已审完。
4. 对 Site 选择一个 SVG／PNG 同源对，确认一次原作及多份导出，而非两项独立内容；对 Viewer 原 catalog 保留其旧 source revision 和派生身份。
5. 对字体、工具样本、运行时 Agent 文本和动态生成世界说明实际来源／范围；没有固定目标数量的内容保留具体未核项，不填零。

通过条件：本文选定集合可重复得到，单位与原来源一致，源包／参考稿没有被写成已采用资产；范围外与未知项仍明确。输出不证明逐项功能、可玩性或发行资产总量。

<a id="cp-rights"></a>
### CP-RIGHTS：核对真实来源材料

选定一项实际内容或具有相同来源的一组内容，先从原 manifest／制作说明读取原作、取得入口、源／导出版本、用途、许可／授权、署名／notice 和编辑限制。取得证据时沿原权限，公开结果只写稳定定位和必要结论；不上传凭据、私人协议正文或不允许再分发的原件。

依次核对：

1. **文件身份。** 对所选实际文件计算 hash；比较候选采用版本与来源材料指向版本。仅链接浮动网页／字体服务或只有文件名时，不能报告固定版本一致。
2. **编辑能力。** 区分 SVG／文本／Rust 等真实编辑源与 raster、WASM、bundle、OBJ 等产物。只收到导出时说明可做哪些修改；源图或原工程无法取得时保留限制。
3. **材料适用。** 将本次公开展示、修改、嵌入／再分发或外包继续制作的实际用途与材料核对。不同用途有不同资料时分别写结论，未知不写成否定或通过。
4. **署名与依赖。** 对照实际交付中的适用 notice、字体／图像／生成输入和原引用；根 LICENSE 不吞并未核第三方材料，外包声明不自动证明内部引用均有来源。
5. **消费结果。** 有来源材料仍需核该候选实际使用的文件；来源核对通过不证明字形、音频／视觉品质或部署通过。

当前字体必须至少分开检查三条加载链：

| 输入 | 可执行的源核查 | 必须保留的未证范围 |
| --- | --- | --- |
| Launcher 的 ms-yahei.ttf | 读 [main.rs](../../../crates/oasis7_client_launcher/src/main.rs) 的 `EGUI_CJK_FONT_BYTES`、`configure_egui_fonts` 和环境变量覆盖；固定所选文件 hash；查看原字体 name 表与实际取得材料 | name 表中的 family／version／manufacturer／license 文字只是文件材料。相邻目录缺少本项目取得或分发依据时，不用元数据或嵌入标志给出权利结论；覆盖字体也须独立核查。 |
| Bevy 默认字体 | 读 [Cargo.toml](../../../crates/pixel_world_bridge/Cargo.toml) 的 `default_font` 和 Cargo.lock 固定版本；从该版本真实依赖包／供应商原入口取得字体原件及 notice，再追实际打包 | 没有取得固定依赖资源就保留未知，不从记忆中的字体名称或另一版 Bevy 推定。 |
| 系统／远程字体 | 对比 Viewer CSS 字体栈、Site CSS 的 `@import` 和传播包请求；真实环境记录解析／返回与 fallback；若本地打包，核其新增文件来源 | 系统名称不等于随包文件，远程服务地址不等于固定字体版本；静态源码不证明用户环境或远程响应可用。 |

静态检查的合格结果可以是“仓库声明已定位；指定第三方用途依据未核实”，不能据此写“全资产权利齐备”。问题回原资产／依赖／发行 owner，保留补证、换源或范围调整的具体去向；不自动修改只读第三方目录或替用户作法律判断。

<a id="cp-handoff"></a>
### CP-HANDOFF：从制作源走到实际接收

选择本次真实改变的类型，沿[类型交接表](content-production-and-rights.design.md#production-handoff)核对：

1. 制作方指出原内容 ID、实际源、规格、相关输入／语言／字体、输出及用途；源与生成物分别定位。
2. 消费方指出已有导入、构建或发布入口，并说明所收版本如何进入实际候选。不存在接入链时，结论停在“可交接源／待集成”，不能编造 loader。
3. 选择原专业手册的规格、语义、目标环境方法。分别核制作方完成了什么、消费方完成了什么，以及何处仍未知。
4. 若只有静态设计推演，检查完整路径与失败 owner 即可，但结果必须保持文档层；需要宣称可用、生产能力或发行时，补真实产物／加载与相应验证。

本设计的代表路径是 `water_ice` 内容键 → Rust palette／shading → bridge WASM → Viewer 实际加载 → 原视觉手册；以及 `first-iron-batch-zh.svg`／英文源 → 对应 PNG → 实际页面／传播包 → 原 Site 核查。前者没有独立贴图原件，后者有可编辑 SVG 也不能自动证明字体与 PNG 的同候选关系。

失败处理：缺源交还制作方；缺导入／源包身份交还集成方；语义、字形、可访问性或素材品质问题交回对应专业 owner；权利材料缺口按 CP-RIGHTS。拿到文件和完成一项构建都不代签整个路径。

<a id="cp-change"></a>
### CP-CHANGE：核对一次上游变化的传播

用本次实际变更，或明确标为纸面的代表样例，依次列出原源／规格、被影响的导出、直接消费者、关联内容和应失效的旧证据。静态审读分别推演以下两例，不必实际改资源：

- `water_ice` 外观变化：组成事实保持原权威；复核 palette、unknown 对照、LOD、bridge 产物、实际加载和可读性证据。只换预览 PNG 不能更新运行图形。
- Launcher 字体替换：原加载路径、所选字体 hash／许可／notice、环境变量覆盖、CJK／拉丁混排、目标平台和包内容一起核对。一个 SVG 换了字体名不等于二进制内嵌资源已替换；旧截图不能代签新字体。

涉及配方／初态则另用[工业 IC-CHANGE](industrial-native-content-delivery.manual.md#ic-change)，保留已有行动和账本的原兼容义务。涉及页面则沿原 Site 纠正方法核正文、派生摘要和历史引用。

通过条件：每个真实受影响消费者有处理或有依据的继续复用结论，旧／新源与实际产物对应，失败有明确修复归属。仅列依赖图不能声明发生了实际试产；运行声明仍须真实候选证据。

<a id="cp-plan"></a>
### CP-PLAN：核批次、角色与外包边界

从实际拟交付体验反查需要的内容 ID、增量数量、语言／平台／品质、正常与失败状态、必要制作与消费角色。按[范围选择](content-production-and-rights.design.md#production-selection)说明复用和剩余范围；不能用删关键失败表达、删来源材料或移走既有验收义务换取“数量完成”。

对外包的双语传播图示例，逐项确认报价到底含不含原文校核、翻译、两个可编辑 SVG、PNG 导出、字体／来源资料、尺寸变体、目标环境核查与返修。报价未含的工作保留内部 owner 和成本；未取得报价不写已采购。所选素材存在限制时先核用途与可交接权限，不能把一份不能继续编辑的 PNG 当作完整制作包。

核对每项必要职能的实际可用输入、容量与依赖；角色可兼任但不能重复计工时。任务安排和采购决定留原 Task／受控记录。本手册只核安排是否具体，不能由角色表推定人员已到位或授权已发生。

<a id="cp-cost"></a>
### CP-COST：校验成本口径与真实产能证据

先核每个输入是实测、报价、估算还是假设。按类别和角色拆 F、E／I／V、J、R、M；确认共享准备、复用、返工、外包含税／计价口径、内部接收与维护窗口没有重复或遗漏。等待单列日历时间；缺失费率不填零。

下列只读算例复核设计 §4.3，数值全是假设，不进行素材生产或付款：

```bash
python3 - <<'PY'
from decimal import Decimal as D

units = [
    (4, (D('.5'), D('.25'), D('.25'))),
    (2, (D('1'), D('1'), D('.5'))),
    (1, (D('1.5'), D('2'), D('1.5'))),
]
fixed = (D('4'), D('3'), D('1'))
joint = (D('0'), D('1'), D('2'))
rework = (D('2'), D('1'), D('1'))
maintenance = (D('1'), D('0'), D('1'))
hours = [sum(D(n) * work[r] for n, work in units)
         + fixed[r] + joint[r] + rework[r] + maintenance[r]
         for r in range(3)]
rates = (D('100'), D('120'), D('100'))
capacity = (D('10'), D('5'), D('10'))
print('hypothetical_role_hours', hours)
print('hypothetical_total_hours', sum(hours))
print('hypothetical_cost', sum(h * p for h, p in zip(hours, rates)) + D('60'))
print('resource_lower_bound_weeks', max(h / c for h, c in zip(hours, capacity)))
PY
```

预期角色人时为 12.5／10／8.5、总计 31、假设费用 3,360、资源下界 2 周。再做三个反例核查：将同一共享准备重复乘 N 应被识别；将供应商已包含的生产人时再次计入内部 H 应被识别；将等待 3 天直接加作 72 人时应被识别。反例只验证口径，不证明真实工期。

实际扩大生产时，从原记录抽取与目标复杂度相符的批次，确认覆盖“制作→转换／导入→集成→目标环境验证”和一次适用规格／依赖变化的传播。分别计算初次建设与后续重复成本、实际返工、维护以及未覆盖类别；既有适用记录可复用，不机械要求每类重复固定次数。没有相应记录时，结论保持“模型可算、产能未验证”。

## 3. 结果解释与收尾

静态文件计数、来源材料审阅、公式正确、实际素材可用、真实生产能力和发行就绪是不同结果。沿原记录保留每项实际范围、失败与未执行项；未完成授权、集成或目标环境验证不因文档 gate 通过而改变。

将真实修正写回原制作／内容 authority，导航只更新有关入口与边界。临时输出按[开发规范的交付与清理](../../engineering/workflow/source-of-truth.md#5-交付、状态与清理)处理；仍被消费者使用的源、正式导出、许可／notice 与不可重复原件保持其原存储和生命周期。
