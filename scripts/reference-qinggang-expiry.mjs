import assert from 'node:assert/strict';
export function installQinggangExpiry() {
  window.__runQinggangExpiry = async ({lib,game,ui,get,_status,actor:A}) => {
    if (!_status.auto) ui.click.auto('forced'); lib.config.game_speed='vvfast';
    const seats=[A]; while(seats.length<8) seats.push(seats.at(-1).nextSeat);
    const B=seats[1],C=seats[2],D=seats[3]; if(game.me===B||game.me===C) game.swapPlayer(A);
    const label=p=>p?'ABCDEFGH'[seats.indexOf(p)]??null:null;
    const tracked=new Map(), events=[], stages=[], calls=[], frames=new WeakMap(); let serial=0,trial=null,activeCard=null,material=null;
    const frame=e=>{if(!e)return null;if(!frames.has(e))frames.set(e,++serial);return frames.get(e)};
    const make=(id,name,suit,rank)=>{const c=game.createCard2(name,suit,rank);tracked.set(c,id);return c};
    const ids=cards=>(cards||[]).flatMap(c=>tracked.has(c)?[tracked.get(c)]:(c.cards||[]).filter(x=>tracked.has(x)).map(x=>tracked.get(x)));
    const zone=c=>{const p=get.owner(c),z=get.position(c,true);return p&&z?`${label(p)}:${z}`:z??null};
    const state=()=>({hp:[B.hp,C.hp],has:[B.hasSkill('qinggang2'),C.hasSkill('qinggang2')],
      contains:[B,C].map(p=>!!activeCard && Array.isArray(p.storage.qinggang2) && p.storage.qinggang2.includes(activeCard)),
      cards:Object.fromEntries([...tracked].map(([c,id])=>[id,zone(c)]))});
    const timings=['useCardBefore','useCardCancelled','useCardToPlayered','useCardToPlayeredCancelled','useCardAfter','useCardEnd',
      'damageBegin1','damage','damageEnd','damageCancelled','damageZero','shaMiss','shaHit','judgeBefore','judgeAfter','useCardToExcluded'];
    const matches=e=>!!trial && !!e.card && (activeCard===e.card || (material && ((e.cards||[]).includes(material) || (e.card.cards||[]).includes(material))));
    const log=(kind,timing,e)=>{if(!timings.includes(timing)||!matches(e))return;
      if(timing==='useCardBefore') activeCard=e.card;
      events.push([trial,kind,timing,frame(e),e.name,frame(e.getParent()),e.getParent()?.name??null,label(e.player),label(e.target),ids(e.cards),state()]);};
    for(const [suffix,priority] of [['high',13],['low',11]]) {
      const skill=`reference_qinggang_${suffix}`;
      lib.skill[skill]={trigger:{global:timings},firstDo:true,priority,forced:true,silent:true,filter:e=>matches(e),
        async content(event,trigger){log(`callback-${suffix}`,event.triggername,trigger)}};
      game.finishSkill(skill);D.addSkill(skill);
    }
    const cancel='reference_qinggang_cancel';
    lib.skill[cancel]={trigger:{global:['damageBegin1','useCardToPlayered']},firstDo:true,priority:14,forced:true,silent:true,
      filter:e=>matches(e)&&((trial==='cancel-damage'&&e.name==='damage'&&e.player===B)||(trial==='cancel-target'&&e.name==='useCardToPlayered'&&e.target===B)),
      async content(event,trigger){trigger.cancel()}};game.finishSkill(cancel);D.addSkill(cancel);
    const boundary='reference_qinggang_boundary';
    lib.skill[boundary]={trigger:{global:'useCardToPlayered'},lastDo:true,forced:true,silent:true,
      filter:e=>matches(e)&&e.player===A&&e.target===(trial==='cancel-root'?C:B)&&['cancel-root','exclude-all'].includes(trial),
      async content(event,trigger){const use=trigger.getParent('useCard',true); if(trial==='cancel-root') use.cancel(); else use.all_excluded=true;}};
    game.finishSkill(boundary);D.addSkill(boundary);
    const zero='reference_qinggang_zero'; lib.skill[zero]={trigger:{global:'damageBegin1'},firstDo:true,priority:14,forced:true,silent:true,
      filter:e=>matches(e)&&trial==='zero-damage'&&e.player===B,async content(event,trigger){trigger.num=0}}; game.finishSkill(zero); D.addSkill(zero);
    for(const name of ['qinggang_skill','bagua_skill'])game.globalEventHandlers.pushHandler(name,(e,{state:step})=>{
      if(trial&&['begin','end'].includes(step)) calls.push([trial,name,step,label(e.player),frame(e),frame(e.getTrigger?.()),state()]);});
    const configured=new WeakSet();
    for(const name of ['chooseToUse','chooseToRespond','chooseBool'])game.globalEventHandlers.pushHandler(name,(e,{state:step})=>{
      if(!trial||step!=='begin'||configured.has(e))return;configured.add(e);
      if(name==='chooseToUse'&&e.getParent()?.name==='phaseUse'){e.ai1=c=>tracked.get(c)==='multi-sha'?20:-10;e.ai2=p=>p===B||p===C?10:-10;}
      if(name==='chooseBool')e.ai=()=>trial!=='miss';
      if(name==='chooseToRespond')e.ai=c=>tracked.get(c)==='shan'?20:-10;
    });
    const proto=lib.element.GameEvent.prototype,original=proto.trigger;
    proto.trigger=function(timing){log('request',timing,this);return original.apply(this,arguments)};
    try{
      for(const p of seats){const held=p.getCards('hej');if(held.length)await p.discard(held)}
      B.hp=B.maxHp=12;C.hp=C.maxHp=12;B.update();C.update();
      const sword=make('sword','qinggang','spade',6),fang=make('fang','fangtian','spade',12),armorB=make('armor-B','bagua','club',2),armorC=make('armor-C','bagua','club',3);
      await A.expandEquip(1);await A.equip(fang);await A.equip(sword);await B.equip(armorB);await C.equip(armorC);
      const multi=make('multi-sha','sha','spade',7);await A.gain(multi);trial='multi';activeCard=null;material=multi;await A.phaseUse();stages.push(['multi',state()]);trial=null;
      await C.discard(C.getCards('e'));B.hp=12;B.update();
      const miss=make('miss-sha','sha','spade',8),shan=make('shan','shan','heart',2);await A.gain(miss);await B.gain(shan);
      trial='miss';activeCard=null;material=miss;await A.useCard(miss,B,false);stages.push(['miss',state()]);trial=null;
      const follow=make('follow-sha','sha','spade',9),red=make('red','sha','heart',9);await C.gain(follow);ui.cardPile.prepend(red);
      trial='follow';activeCard=null;material=follow;await C.useCard(follow,B,false);stages.push(['follow',state()]);trial=null;
      const cancelled=make('cancel-damage-sha','sha','spade',10);await A.gain(cancelled);trial='cancel-damage';activeCard=null;material=cancelled;
      await A.useCard(cancelled,B,false);stages.push(['cancel-damage',state()]);trial=null;
      const target=make('cancel-target-sha','sha','spade',11);await A.gain(target);trial='cancel-target';activeCard=null;material=target;
      await A.useCard(target,B,false);stages.push(['cancel-target',state()]);trial=null;
      const zeroSha=make('zero-sha','sha','spade',12);await A.gain(zeroSha);trial='zero-damage';activeCard=null;material=zeroSha;
      await A.useCard(zeroSha,B,false);stages.push(['zero-damage',state()]);trial=null;
      for(const name of ['exclude-all','cancel-root']) {
        const attack=make(`${name}-sha`,'sha','spade',13);await A.gain(attack);trial=name;activeCard=null;material=attack;
        await A.useCard(attack,name==='cancel-root'?C:B,false);stages.push([name,state()]);trial=null;
      }
      return{columns:['trial','kind','timing','frame','event','parentId','parent','player','target','cards','state'],events,stages,calls};
    }finally{proto.trigger=original;D.removeSkill(['reference_qinggang_high','reference_qinggang_low',cancel,zero,boundary]);}
  };
}
export function checkQinggangExpiry(result) {
  const rows=(trial,timing,kind)=>result.events.filter(row=>row[0]===trial&&row[2]===timing&&row[1]===kind);
  const stage=name=>result.stages.find(row=>row[0]===name)[1];
  const state=row=>[row[10].hp,row[10].has,row[10].contains];
  const high=rows('multi','damage','callback-high'), low=rows('multi','damage','callback-low');
  assert.deepEqual(high.map(state), [[[11,12],[true,true],[true,true]], [[11,11],[false,true],[false,true]]]);
  assert.deepEqual(low.map(state), [[[11,12],[false,true],[false,true]], [[11,11],[false,false],[false,false]]]);
  for(const [trial,timing] of [['miss','shaMiss'],['cancel-damage','damageCancelled'],['zero-damage','damageZero']]) {
    assert.deepEqual(rows(trial,timing,'callback-high').map(row=>row[10].contains), [[true,false]]);
    assert.deepEqual(rows(trial,timing,'callback-low').map(row=>row[10].contains), [[false,false]]);
  }
  assert.equal(stage('miss').cards.shan,'d');
  assert.equal(stage('follow').cards.red,'d');
  assert.equal(result.calls.filter(row=>row[0]==='follow'&&row[1]==='bagua_skill'&&row[2]==='begin').length,1);
  assert.deepEqual(stage('follow').hp,[12,11]);
  assert.deepEqual(stage('cancel-damage').hp,[12,11]);
  assert.deepEqual(stage('zero-damage').hp,[12,11]);
  assert.deepEqual(stage('cancel-target').has,[false,false]);
  assert.equal(result.calls.filter(row=>row[0]==='cancel-target'&&row[1]==='qinggang_skill'&&row[2]==='begin').length,0);
  assert.equal(result.calls.filter(row=>row[0]==='cancel-target'&&row[1]==='bagua_skill'&&row[2]==='begin').length,1);
  assert.deepEqual(rows('exclude-all','useCardBefore','request')[0][10].has,[false,false]);
  assert.deepEqual(stage('exclude-all').contains,[true,false]);
  assert.deepEqual(rows('cancel-root','useCardBefore','request')[0][10].contains,[false,false]);
  assert.deepEqual(stage('cancel-root').contains,[false,true]);
  assert.equal(rows('cancel-root','useCardCancelled','request').length,1);
  assert.equal(stage('cancel-root').cards['cancel-root-sha'],'d');
  for(const row of result.events) { assert.ok(row[3] && row[5], `event and parent IDs: ${row[0]} ${row[2]}`); }
}
