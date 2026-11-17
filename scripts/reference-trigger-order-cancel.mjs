// Installed in the upstream page; all rule operations remain upstream.
export async function installTriggerOrderScenario({ actor, target, game, lib }) {
  let material, choiceEvent;
  const skillSequence = [];
  const filterResults = [];
  material = game.createCard2('tao', 'heart', 2);
  await target.gain(material, 'gain2');
  const record = (name, event, trigger, player) => skillSequence.push({
    skill: name, player: player === actor ? 'A' : 'B', event: event.name,
    parent: event.getParent()?.name || null, trigger: trigger.name,
    triggerParent: trigger.getParent()?.name || null,
  });
  const skill = (name, extra = {}) => ({
    trigger: { source: 'damageBegin4' }, forced: true, ...extra,
    async content(event, trigger, player) { record(name, event, trigger, player); },
  });
  lib.skill.reference_order_first = skill('first', { firstDo: true });
  lib.skill.reference_order_alpha = skill('alpha');
  lib.skill.reference_order_beta = skill('beta');
  lib.skill.reference_order_filtered = skill('filtered', { priority: -1, filter: () => {
    const eligible = !window.__referenceFiltered;
    filterResults.push(eligible);
    return eligible;
  } });
  lib.skill.reference_order_beta.content = async (event, trigger, player) => {
    record('beta', event, trigger, player);
    window.__referenceFiltered = true;
  };
  lib.skill.reference_order_cost = {
    trigger: { player: 'damageBegin4' }, priority: 100,
    async cost(event, trigger, player) {
      record('cost', event, trigger, player);
      await player.discard(material);
      event.result = { bool: true };
    },
    async content(event, trigger, player) { record('target', event, trigger, player); },
  };
  lib.skill.reference_order_last = skill('last', { lastDo: true, trigger: { global: 'damageBegin4' }, filter: event => event.player === target && event.source === actor });
  lib.skill.reference_order_last.content = async (event, trigger, player) => {
    record('last', event, trigger, player);
    trigger.cancel();
  };
  for (const [id, title] of Object.entries({
    reference_order_first: 'First', reference_order_alpha: 'Alpha', reference_order_beta: 'Beta',
    reference_order_filtered: 'Filtered', reference_order_cost: 'Target cost', reference_order_last: 'Last',
  })) {
    lib.translate[id] = title;
    lib.translate[`${id}_info`] = title;
    game.finishSkill(id);
  }
  actor.addSkill(['reference_order_first', 'reference_order_alpha', 'reference_order_beta', 'reference_order_filtered', 'reference_order_last']);
  target.addSkill('reference_order_cost');
  const originalChooseControl = actor.chooseControl;
  actor.chooseControl = function (...args) {
    const next = originalChooseControl.apply(this, args);
    if (args[0]?.includes('Alpha') && args[0]?.includes('Beta')) {
      choiceEvent = next;
      next.set('ai', () => 1);
    }
    return next;
  };
  return { material, skillSequence, filterResults, getChoiceEvent: () => choiceEvent };
}
