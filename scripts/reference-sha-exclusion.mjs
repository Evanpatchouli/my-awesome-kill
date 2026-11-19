import assert from 'node:assert/strict';
export function installShaExclusion() {
  window.__runShaExclusion=async({lib,game,ui,get,_status,actor:A})=>{
    if(!_status.auto)ui.click.auto('forced');lib.config.game_speed='vvfast';
    const seats=[A];while(seats.length<8)seats.push(seats.at(-1).nextSeat);const B=seats[1],C=seats[2],D=seats[3];
    if(game.me===B||game.me===C)game.swapPlayer(A);
    const label=p=>p?'ABCDEFGH'[seats.indexOf(p)]??null:null,tracked=new Map(),events=[],stages=[],calls=[],frames=new WeakMap();
    let serial=0,trial=null,outer=null,inner=null,outerV=null,innerV=null,mode=null;
    const frame=e=>{if(!e)return null;if(!frames.has(e))frames.set(e,++serial);return frames.get(e)};
    const make=(id,name,suit,rank)=>{const c=game.createCard2(name,suit,rank);tracked.set(c,id);return c};
    const ids=cards=>(cards||[]).flatMap(c=>tracked.has(c)?[tracked.get(c)]:(c.cards||[]).filter(x=>tracked.has(x)).map(x=>tracked.get(x)));
    const zone=c=>{const p=get.owner(c),z=get.position(c,true);return p&&z?`${label(p)}:${z}`:z??null};
    const which=e=>e.card===outerV||[outer].some(c=>c&&((e.cards||[]).includes(c)||(e.card?.cards||[]).includes(c)))?'outer':
      e.card===innerV||[inner].some(c=>c&&((e.cards||[]).includes(c)||(e.card?.cards||[]).includes(c)))?'inner':null;
    const state=()=>({hp:[B.hp,C.hp],has:[B,C].map(p=>p.hasSkill('qinggang2')),
      contains:[B,C].map(p=>!!outerV&&Array.isArray(p.storage.qinggang2)&&p.storage.qinggang2.includes(outerV)),
      cards:Object.fromEntries([...tracked].filter(([c])=>c===outer||c===inner||['sword','fang','armor-B','armor-C'].includes(tracked.get(c))).map(([c,id])=>[id,zone(c)]))});
    const bases=['useCard2','useCard','useCardToPlayer','useCardToTarget','useCardToPlayered','useCardToTargeted','useCardToExcluded','sha','damage','shaMiss'];
    const timings=[...new Set(bases.flatMap(n=>[n,n+'Before',n+'Begin',n+'End',n+'After',n+'Cancelled']))];
    const log=(kind,timing,e)=>{if(!trial||!timings.includes(timing)||!which(e))return;
      if(e.name==='useCard'&&e.card){if(which(e)==='outer')outerV=e.card;else innerV=e.card;}
      events.push([trial,kind,timing,which(e),frame(e),e.name,frame(e.getParent()),e.getParent()?.name??null,label(e.player),label(e.target),ids(e.cards),!!e.finished,!!e._cancelled,state()]);};
    const registered=[];
    for(const [group,priority] of [['first-high',15],['first-low',-15],['normal-high',15],['normal-low',-15],['last-high',15],['last-low',-15]]){
      const id=`reference_exclusion_${group}`,first=group.startsWith('first'),last=group.startsWith('last');
      lib.skill[id]={trigger:{global:timings},firstDo:first,lastDo:last,priority,forced:true,silent:true,
        filter:e=>!!trial&&!!which(e),async content(event,trigger){log(group,event.triggername,trigger)}};game.finishSkill(id);D.addSkill(id);registered.push(id);
    }
    for(const timing of ['useCard2','useCard','useCardToPlayered'])for(const group of ['first','normal','last']){
      const id=`reference_exclusion_cancel_${timing}_${group}`;
      lib.skill[id]={trigger:{global:timing},firstDo:group==='first',lastDo:group==='last',priority:0,forced:true,silent:true,
        filter:e=>!!mode&&mode.type==='cancel'&&mode.group===group&&mode.timing===timing&&which(e)==='outer'&&(e.name!=='useCardToPlayered'||e.target===B),
        async content(event,trigger){log(`cancel-${group}`,event.triggername,trigger);const root=trigger.name==='useCard'?trigger:trigger.getParent('useCard',true);root.cancel();}};
      game.finishSkill(id);D.addSkill(id);registered.push(id);
    }
    const alter='reference_exclusion_target';
    lib.skill[alter]={trigger:{global:'useCardToPlayered'},lastDo:true,priority:0,forced:true,silent:true,
      filter:e=>!!mode&&mode.type==='exclude'&&which(e)==='outer'&&((mode.which==='B'&&e.target===B)||(mode.which==='C'&&e.target===C)||mode.which==='each'||(mode.which==='all'&&e.target===B)||(mode.which==='nested'&&e.target===B)),
      async content(event,trigger){log(`exclude-${mode.which}`,event.triggername,trigger);const root=trigger.getParent('useCard',true);
        if(mode.which==='all')root.all_excluded=true;
        else {trigger.excluded.add(trigger.target);if(mode.which==='nested')await C.useCard(inner,B,false);}}};
    game.finishSkill(alter);D.addSkill(alter);registered.push(alter);
    for(const name of ['qinggang_skill','bagua_skill'])game.globalEventHandlers.pushHandler(name,(e,{state:step})=>{
      if(trial&&['begin','end'].includes(step))calls.push([trial,name,step,label(e.player),frame(e),frame(e.getTrigger?.()),state()]);});
    const proto=lib.element.GameEvent.prototype,original=proto.trigger;
    proto.trigger=function(timing){log('request',timing,this);return original.apply(this,arguments)};
    try{
      for(const p of seats){const held=p.getCards('hej');if(held.length)await p.discard(held)}
      const sword=make('sword','qinggang','spade',6),fang=make('fang','fangtian','spade',12),armorB=make('armor-B','bagua','club',2),armorC=make('armor-C','bagua','club',3);
      await A.expandEquip(1);await A.equip(fang);await A.equip(sword);await B.equip(armorB);await C.equip(armorC);
      const run=async(name,configuration)=>{
        // Reset prior trial's temporary skill with the public removeSkill API; the reset is outside the recorded trial.
        B.removeSkill('qinggang2');C.removeSkill('qinggang2');B.hp=B.maxHp=20;C.hp=C.maxHp=20;B.update();C.update();
        outer=make(`${name}-sha`,'sha','spade',7);inner=null;outerV=innerV=null;await A.gain(outer);
        if(configuration.which==='nested'){inner=make(`${name}-inner`,'sha','spade',9);await C.gain(inner)}
        trial=name;mode=configuration;await A.useCard(outer,[B,C],false);stages.push([name,state()]);trial=null;mode=null;
      };
      for(const timing of ['useCard2','useCard','useCardToPlayered'])for(const group of ['first','normal','last'])
        await run(`cancel-${timing}-${group}`,{type:'cancel',timing,group});
      for(const which of ['B','C','each','all','nested'])await run(`exclude-${which}`,{type:'exclude',which});
      return{columns:['trial','kind','timing','card','frame','event','parentId','parent','player','target','cards','finished','cancelled','state'],events,stages,calls};
    }finally{proto.trigger=original;D.removeSkill(registered)}
  };
}
export function checkShaExclusion(result) {
  const rows=(trial)=>result.events.filter(row=>row[0]===trial);
  const req=(trial,timing)=>rows(trial).filter(row=>row[1]==='request'&&row[2]===timing);
  const stage=name=>result.stages.find(row=>row[0]===name)[1];
  for(const timing of ['useCard2','useCard','useCardToPlayered']) for(const group of ['first','normal','last']) {
    const trial=`cancel-${timing}-${group}`, events=rows(trial);
    assert.equal(events.filter(row=>row[1]===`cancel-${group}`&&row[2]===timing).length,1,trial);
    const request=req(trial,'useCardCancelled');assert.equal(request.length,1,trial);
    assert.deepEqual(request[0].slice(11,13),[false,true]);
    const callback=events.find(row=>row[1]==='first-high'&&row[2]==='useCardCancelled');
    assert.deepEqual(callback.slice(11,13),[true,true]);
    assert.ok(events.indexOf(request[0])>events.findIndex(row=>row[1]===`cancel-${group}`&&row[2]===timing));
    assert.ok(events.indexOf(callback)>events.indexOf(request[0]));
    assert.equal(stage(trial).cards[`${trial}-sha`],'d');
    assert.ok(events.some(row=>row[13].cards[`${trial}-sha`]==='o'));
    assert.deepEqual(stage(trial).hp,[20,20]);
    assert.equal(req(trial,'useCardEnd').length,0);
  }
  const first=rows('cancel-useCardToPlayered-first');
  const at=(kind,timing)=>first.findIndex(row=>row[1]===kind&&row[2]===timing);
  assert.ok(at('cancel-first','useCardToPlayered')<at('request','useCardCancelled'));
  assert.ok(at('request','useCardCancelled')<at('first-low','useCardToPlayered'));
  assert.ok(at('last-low','useCardToPlayered')<at('first-high','useCardCancelled'));
  assert.deepEqual(first[at('request','useCardCancelled')][13].has,[false,false]);
  assert.deepEqual(first[at('first-high','useCardCancelled')][13].has,[true,false]);
  const excluded=trial=>req(trial,'useCardToExcluded').map(row=>row[9]);
  assert.deepEqual(excluded('exclude-B'),['B']);assert.deepEqual(excluded('exclude-C'),['C']);
  assert.deepEqual(excluded('exclude-each'),['B','C']);assert.deepEqual(excluded('exclude-all'),[]);
  assert.deepEqual(stage('exclude-B').hp,[20,19]);assert.deepEqual(stage('exclude-C').hp,[19,20]);
  assert.deepEqual(stage('exclude-each').hp,[20,20]);assert.deepEqual(stage('exclude-all').hp,[20,20]);
  assert.deepEqual(req('exclude-all','useCardToPlayered').map(row=>row[9]),['B']);
  assert.deepEqual(stage('exclude-nested').hp,[19,19]);
  const nested=req('exclude-nested','useCardToPlayered');
  assert.deepEqual(nested.map(row=>[row[3],row[8],row[9]]),[['outer','A','B'],['inner','C','B'],['outer','A','C']]);
  assert.notEqual(nested[0][6],nested[1][6]);assert.equal(nested[0][6],nested[2][6]);
  assert.equal(stage('exclude-nested').cards['exclude-nested-inner'],'d');
  for(const row of result.events)assert.ok(row[4]&&row[6],`event parent ${row[0]} ${row[2]}`);
}
