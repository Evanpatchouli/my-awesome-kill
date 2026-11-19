import assert from 'node:assert/strict';
export function installQinggangSources() {
  window.__runQinggangSources = async ({lib,game,ui,get,_status,actor:A}) => {
    if (!_status.auto) ui.click.auto('forced');lib.config.game_speed='vvfast';
    const seats=[A];while(seats.length<8)seats.push(seats.at(-1).nextSeat);const B=seats[1],C=seats[2],D=seats[3];
    if(game.me===B||game.me===C)game.swapPlayer(A);
    const label=p=>p?'ABCDEFGH'[seats.indexOf(p)]??null:null, tracked=new Map(),events=[],stages=[],calls=[],frames=new WeakMap();
    let serial=0,trial=null,activeCard=null,material=null;
    const frame=e=>{if(!e)return null;if(!frames.has(e))frames.set(e,++serial);return frames.get(e)};
    const make=(id,name,suit,rank)=>{const c=game.createCard2(name,suit,rank);tracked.set(c,id);return c};
    const ids=cards=>(cards||[]).flatMap(c=>tracked.has(c)?[tracked.get(c)]:(c.cards||[]).filter(x=>tracked.has(x)).map(x=>tracked.get(x)));
    const zone=c=>{const p=get.owner(c),z=get.position(c,true);return p&&z?`${label(p)}:${z}`:z??null};
    const state=()=>({hp:B.hp,out:B.isOut(),has:B.hasSkill('qinggang2'),own:B.skills.includes('qinggang2'),
      temp:Object.hasOwn(B.tempSkills,'qinggang2'),expiry:B.tempSkills.qinggang2??null,
      additional:(B.additionalSkills.reference_source||[]).slice(),mark:!!B.marks.qinggang2,
      hookDamage:(lib.hook[`${B.playerid}_player_damage`]||[]).includes('qinggang2'),
      hookUseEnd:(lib.hook[`${B.playerid}_global_useCardEnd`]||[]).includes('qinggang2'),
      storageLength:Array.isArray(B.storage.qinggang2)?B.storage.qinggang2.length:null,
      contains:!!activeCard&&Array.isArray(B.storage.qinggang2)&&B.storage.qinggang2.includes(activeCard),
      cards:Object.fromEntries([...tracked].map(([c,id])=>[id,zone(c)]))});
    const timings=['useCardBefore','useCardToPlayered','useCardEnd','useCardAfter','damage','damageEnd','shaMiss','judgeBefore','judgeAfter','phaseBeforeStart','phaseAfter'];
    const matches=e=>!!trial && (e.name==='phase'||!!e.card&&(e.card===activeCard||material&&((e.cards||[]).includes(material)||(e.card.cards||[]).includes(material))));
    const record=(kind,timing,e)=>{if(!timings.includes(timing)||!matches(e))return;if(timing==='useCardBefore')activeCard=e.card;
      events.push([trial,kind,timing,frame(e),e.name,frame(e.getParent()),e.getParent()?.name??null,label(e.player),label(e.target),ids(e.cards),state()]);};
    for(const [suffix,priority] of [['high',13],['low',11]]){const name=`reference_source_${suffix}`;
      lib.skill[name]={trigger:{global:timings},firstDo:true,priority,forced:true,silent:true,filter:e=>matches(e),
        async content(event,trigger){record(`callback-${suffix}`,event.triggername,trigger)}};game.finishSkill(name);D.addSkill(name);}
    const out='reference_qinggang_target_out';
    lib.skill[out]={trigger:{global:'useCardToPlayered'},lastDo:true,forced:true,silent:true,
      filter:e=>trial==='out'&&e.player===A&&e.target===B,async content(){B.out()}};game.finishSkill(out);D.addSkill(out);
    const marked='reference_qinggang_marked'; lib.skill[marked]={trigger:{global:'useCardToPlayered'},lastDo:true,forced:true,silent:true,
      filter:e=>['ordinary','additional','temp-hit','temp-refresh-hit'].includes(trial)&&e.player===A&&e.target===B,
      async content(){stages.push([`${trial}-marked`,state()])}};game.finishSkill(marked);D.addSkill(marked);
    for(const name of ['qinggang_skill','bagua_skill','qinggang2'])game.globalEventHandlers.pushHandler(name,(e,{state:step})=>{
      if(trial&&['begin','end'].includes(step))calls.push([trial,name,step,label(e.player),frame(e),frame(e.getTrigger?.()),state()]);});
    game.globalEventHandlers.pushHandler('chooseBool',(e,{state:step})=>{if(trial&&step==='begin')e.ai=()=>true});
    const proto=lib.element.GameEvent.prototype,original=proto.trigger;
    proto.trigger=function(timing){record('request',timing,this);return original.apply(this,arguments)};
    const snapshot=name=>stages.push([name,state()]);
    const use=async(name,source=A)=>{const sha=make(`${name}-sha`,'sha','spade',7);await source.gain(sha);trial=name;material=sha;activeCard=null;await source.useCard(sha,B,false);snapshot(name);trial=null;};
    try{
      for(const p of seats){const held=p.getCards('hej');if(held.length)await p.discard(held)}
      B.hp=B.maxHp=20;B.update();const sword=make('sword','qinggang','spade',6),armor=make('armor','bagua','club',2);
      await B.equip(armor);
      B.addTempSkill('qinggang2');snapshot('independent-before');
      const red=make('independent-red','sha','heart',9);ui.cardPile.prepend(red);await use('independent');
      B.removeSkill('qinggang2');snapshot('independent-removed');
      await A.equip(sword);
      B.addSkill('qinggang2');snapshot('ordinary-before');await use('ordinary');snapshot('ordinary-after');B.removeSkill('qinggang2');
      B.addAdditionalSkill('reference_source','qinggang2');snapshot('additional-before');await use('additional');snapshot('additional-after');B.removeAdditionalSkill('reference_source');
      B.addTempSkill('qinggang2');snapshot('temp-before');await use('temp-hit');snapshot('temp-hit-after');
      trial='temp-phase';material=null;activeCard=null;await B.phase();snapshot('temp-phase-after');trial=null;
      B.addTempSkill('qinggang2');snapshot('temp-refresh-before');await use('temp-refresh-hit');snapshot('temp-refresh-hit-after');
      trial='temp-refresh-phase';material=null;activeCard=null;await B.phase();snapshot('temp-refresh-phase-after');trial=null;
      B.removeSkill('qinggang2');
      B.addTempSkill('qinggang2');snapshot('temp-phase-first-before');
      trial='temp-phase-first';material=null;activeCard=null;await B.phase();snapshot('temp-phase-first-after');trial=null;
      await use('temp-phase-first-hit');
      const heldB=B.getCards('h');if(heldB.length)await B.discard(heldB);
      await use('out');snapshot('out-before-return');B.in();snapshot('out-after-return');
      const followRed=make('out-follow-red','sha','heart',10);ui.cardPile.prepend(followRed);await use('out-follow',C);
      return {columns:['trial','kind','timing','frame','event','parentId','parent','player','target','cards','state'],events,stages,calls};
    }finally{proto.trigger=original;D.removeSkill(['reference_source_high','reference_source_low',out,marked]);}
  };
}
export function checkQinggangSources(result) {
  const stage=name=>result.stages.find(row=>row[0]===name)[1];
  const calls=(trial,skill)=>result.calls.filter(row=>row[0]===trial&&row[1]===skill&&row[2]==='begin');
  const rows=(trial,timing)=>result.events.filter(row=>row[0]===trial&&row[1]==='request'&&row[2]===timing);
  assert.deepEqual([stage('independent-before').has,stage('independent-before').temp,stage('independent-before').storageLength],[true,true,0]);
  assert.deepEqual([stage('independent').hp,stage('independent').cards['independent-red'],calls('independent','bagua_skill').length],[19,'c',0]);
  for(const source of ['ordinary','additional']) {
    assert.deepEqual([stage(`${source}-marked`).mark,stage(`${source}-marked`).contains],[true,true]);
    const after=stage(`${source}-after`);
    assert.deepEqual([after.has,after.own,after.temp,after.additional,after.mark,after.hookDamage,after.hookUseEnd],[false,false,false,[],false,false,false]);
    assert.equal(calls(source,'qinggang2').length,1);
  }
  assert.equal(stage('ordinary-before').own,true);
  assert.deepEqual(stage('additional-before').additional,['qinggang2']);
  const expiry={global:['phaseAfter','phaseBeforeStart']};
  for(const trial of ['temp-hit','temp-refresh-hit']) {
    const marked=stage(`${trial}-marked`),after=stage(`${trial}-after`);
    assert.deepEqual([marked.temp,marked.expiry,marked.mark,marked.contains],[true,expiry,true,true]);
    assert.deepEqual([after.has,after.temp,after.mark,after.hookDamage],[false,false,false,false]);
  }
  assert.deepEqual(stage('temp-phase-first-before').expiry,expiry);
  assert.deepEqual([stage('temp-phase-first-after').has,stage('temp-phase-first-after').temp],[false,false]);
  assert.equal(rows('temp-phase-first','phaseAfter').length,1);
  assert.deepEqual([stage('out-before-return').out,stage('out-before-return').has,stage('out-before-return').mark,stage('out-before-return').contains],[true,true,true,true]);
  assert.deepEqual([stage('out-after-return').out,stage('out-after-return').has,stage('out-after-return').contains],[false,true,true]);
  assert.equal(stage('out-follow').hp,stage('out-after-return').hp-1);
  assert.equal(stage('out-follow').cards['out-follow-red'],'c');
  assert.equal(calls('out-follow','bagua_skill').length,0);
  for(const row of result.events) assert.ok(row[3]&&row[5], `event parent missing ${row[0]} ${row[2]}`);
}
