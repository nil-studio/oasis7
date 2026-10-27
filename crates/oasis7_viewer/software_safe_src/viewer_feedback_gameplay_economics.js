function normalizeGameplayToken(value) {
  return String(value || "")
    .trim()
    .toLowerCase()
    .replaceAll("_", "")
    .replaceAll("-", "");
}

export function buildGameplayEconomicSurface({
  locale,
  localeText,
  gameplay,
  availableActions,
  recommendedAction,
  recentFeedback,
  blockerLabel,
  narrativeNextStep,
  lastWorldChange,
}) {
  const goalKind = normalizeGameplayToken(gameplay.goal_kind);
  const blockerKind = normalizeGameplayToken(gameplay.blocker_kind);
  const blockerDetail = gameplay.blocker_detail || recentFeedback?.reason || null;
  const fallbackLabel = gameplay.fallback_action_label || recommendedAction?.label || null;

  const input = (() => {
    if (blockerKind === "materialshortage") {
      return localeText(
        locale,
        blockerDetail
          ? `当前关键投入缺口是物料链：${blockerDetail}`
          : "当前关键投入缺口是物料链，先把原料重新接上。",
        blockerDetail
          ? `The gating input is the material chain: ${blockerDetail}`
          : "The gating input is the material chain; restore raw material flow first.",
      );
    }
    if (blockerKind === "powershortage") {
      return localeText(
        locale,
        blockerDetail
          ? `当前关键投入缺口是供电：${blockerDetail}`
          : "当前关键投入缺口是供电，先恢复能量再谈扩产。",
        blockerDetail
          ? `The gating input is power availability: ${blockerDetail}`
          : "The gating input is power availability; restore energy before expanding.",
      );
    }
    if (blockerKind === "governancegate") {
      return localeText(
        locale,
        blockerDetail
          ? `当前关键投入缺口是许可或治理前提：${blockerDetail}`
          : "当前关键投入缺口是许可或治理前提，先补齐访问资格。",
        blockerDetail
          ? `The gating input is permission/governance: ${blockerDetail}`
          : "The gating input is permission/governance; satisfy the access prerequisite first.",
      );
    }
    if (goalKind === "createfirstworldfeedback") {
      return localeText(
        locale,
        "当前投入不是更多库存，而是 1 次 committed world step 加 1 次可读 delta。",
        "The current input is not more inventory; it is one committed world step plus one readable delta.",
      );
    }
    if (goalKind === "startfactoryrun") {
      return localeText(
        locale,
        "当前投入是能持续一个完整周期的配方、原料和供电，而不是一次性点亮。",
        "The current input is a recipe, materials, and power that can survive one full cycle, not a one-off ignition.",
      );
    }
    if (goalKind === "turnmaterialflowintooutput") {
      return localeText(
        locale,
        "当前投入是把原料流真正推过产线，直到它变成首个制成品。",
        "The current input is pushing material flow all the way through the line until it becomes first finished output.",
      );
    }
    if (goalKind === "stabilizefirstline" || goalKind === "establishfirstcapability") {
      return localeText(
        locale,
        "当前投入是让第一条线能扛住一次中断并恢复，而不是只完成一次幸运产出。",
        "The current input is making the first line survive one interruption and recover, not just finishing one lucky output.",
      );
    }
    if (goalKind === "choosefirstexpansiontradeoff" || goalKind === "choosemidlooppath") {
      return localeText(
        locale,
        "当前投入是一条已证明可用的能力线，以及一个值得付出机会成本的新分支。",
        "The current input is one proven capability line plus a branch worth its opportunity cost.",
      );
    }
    return gameplay.objective
      || gameplay.progress_detail
      || localeText(
        locale,
        "当前还没有发布更细的经济投入说明。",
        "No finer-grained economic input explanation is published yet.",
      );
  })();

  const output = lastWorldChange
    || recentFeedback?.effect
    || gameplay.progress_detail
    || localeText(
      locale,
      "当前还没有新的世界级结果；先看阻塞与下一步。",
      "There is no new world-level result yet; read the blocker and next step first.",
    );

  const unlockedValue = (() => {
    if (goalKind === "createfirstworldfeedback") {
      return localeText(
        locale,
        "一旦看见第一条 committed delta，你拿到的是“我的命令真的会改世界”的信任，而不是单纯一条日志。",
        "Once the first committed delta lands, you gain trust that your command truly changes the world, not just another log line.",
      );
    }
    if (goalKind === "recovercapability") {
      return localeText(
        locale,
        "修复后恢复的是已有能力位，而不是被迫从旁观状态重开一条完全新线。",
        "Repair restores an existing capability slot instead of forcing you to restart from a watch-only state.",
      );
    }
    if (goalKind === "startfactoryrun" || goalKind === "turnmaterialflowintooutput") {
      return localeText(
        locale,
        "这一拍的新用途，是把原料和站点从“摆着”变成“能稳定产出下一种东西”。",
        "The new use here is turning idle materials and a site into something that can reliably produce the next thing.",
      );
    }
    if (goalKind === "stabilizefirstline" || goalKind === "establishfirstcapability") {
      return localeText(
        locale,
        "这一拍的新用途，是把一次性成果升级成可重复调用的能力位与恢复弹性。",
        "The new use here is upgrading a one-off success into a reusable capability slot with recovery elasticity.",
      );
    }
    if (goalKind === "choosefirstexpansiontradeoff" || goalKind === "choosemidlooppath") {
      return localeText(
        locale,
        "这一拍的新用途，是给你一个真正不同的增长或专业化分支，而不是继续重复同一循环。",
        "The new use here is unlocking a genuinely different growth or specialization branch instead of repeating the same loop.",
      );
    }
    return localeText(
      locale,
      "当前系统已经在尝试把“继续推进”解释成新的 leverage，而不是更多库存数字。",
      "The system is trying to frame this step as new leverage, not just bigger stockpile numbers.",
    );
  })();

  // Retain the wire adapter name; this is observation/action guidance, not
  // a domain assessment or a promise that a repair is available.
  const repairAction = (() => {
    if (fallbackLabel) {
      return blockerDetail
        ? localeText(
          locale,
          `${fallbackLabel}，然后检查提交结果及 blocker 是否变化。`,
          `${fallbackLabel}, then inspect the committed result and whether the blocker changed.`,
        )
        : fallbackLabel;
    }
    return narrativeNextStep
      || localeText(
        locale,
        "请先读取下一步指引并检查当前状态。",
        "Read the next-step guidance and inspect the current state.",
      );
  })();

  const nextValue = (() => {
    if (gameplay.branch_hint) {
      return gameplay.branch_hint;
    }
    if (goalKind === "recovercapability") {
      return localeText(
        locale,
        "完成这次修复后，停住的产线会重新变成可经营能力。",
        "Once this repair holds, the stalled line becomes an operable capability again.",
      );
    }
    if (goalKind === "stabilizefirstline" || goalKind === "establishfirstcapability") {
      return localeText(
        locale,
        "稳定性会把一次成功变成后续扩张、恢复或分工的前提。",
        "Stability turns one success into the prerequisite for expansion, recovery, or specialization.",
      );
    }
    if (goalKind === "choosefirstexpansiontradeoff" || goalKind === "choosemidlooppath") {
      return localeText(
        locale,
        "下一步会改变你拿到的杠杆类型，而不只是把同一种产出做得更多。",
        "The next move changes the kind of leverage you get, not just the amount of the same output.",
      );
    }
    if (goalKind === "createfirstworldfeedback") {
      return localeText(
        locale,
        "先确认第一条世界反馈，后面的工业选择才不再像盲按按钮。",
        "Confirm the first world feedback so later industrial choices stop feeling blind.",
      );
    }
    return narrativeNextStep
      || localeText(
        locale,
        "下一步应该带来新的用途、恢复弹性或更清晰的分支价值。",
        "The next move should create new use, recovery elasticity, or a clearer branch value.",
      );
  })();

  return {
    input,
    output,
    unlockedValue,
    repairAction,
    nextValue,
    blockerLabel: blockerLabel || null,
  };
}
