import assert from 'node:assert/strict';
export function installCancelDeferred() {
  window.__runCancelDeferred=async({lib,game,ui,get,_status,actor:A})=>{
    if(!_status.auto)ui.click.auto('forced');lib.config.game_speed='vvfast';
    const seats=[A];while(seats.length<8)seats.push(seats.at(-1).nextSeat);const B=seats[1],D=seats[3],E=seats[4];
    if(game.me===B||game.me===D)game.swapPlayer(A);
    const tracked=new Map(),events=[],stages=[],frames=new WeakMap();let next=0,trial=null,outer=null,card=null,indexCount=0,expansion=null;
    const frame=e=>{if(!e)return null;if(!frames.has(e))frames.set(e,++next);return frames.get(e)};
    const label=p=>p?'ABCDEFGH'[seats.indexOf(p)]??null:null;
    const zone=c=>{const p=get.owner(c),z=get.position(c,true);return p&&z?`${label(p)}:${z}`:z??null};
    const state=()=>({out:D.isOut(), index:indexCount, expired:!B.hasSkill('reference_deferred_expire'),
      installed:D.hasSkill('reference_deferred_existing'),late:E.hasSkill('reference_deferred_late'),indexed:D.hasSkill('reference_deferred_index'),
      expansion:expansion?zone(expansion):null,mark:B.hasSkill('qinggang2'),contains:!!card&&Array.isArray(B.storage.qinggang2)&&B.storage.qinggang2.includes(card),material:outer?zone(outer):null});
    const timings=['useCardToPlayered','useCardCancelled','useCardEnd','useCardAfter','loseBefore','loseBegin','loseEnd','loseAfter','gainBefore','gainBegin','gainEnd','gainAfter','loseToDiscardpileBefore','loseToDiscardpileEnd','loseToDiscardpileAfter'];
    const matches=e=>!!trial&&((!!e.card&&(e.card===card||outer&&((e.cards||[]).includes(outer)||(e.card.cards||[]).includes(outer))))||(expansion&&(e.cards||[]).includes(expansion)));
    const record=(kind,timing,e,detail=null)=>{if(!matches(e))return;if(!card && e.card)card=e.card;events.push([trial,kind,timing,frame(e),e.name,frame(e.getParent()),e.getParent()?.name??null,label(e.player),label(e.target),detail,state(),(e.cards||[]).map(c=>tracked.get(c)).filter(Boolean)]);};
    const existing='reference_deferred_existing',late='reference_deferred_late',indexed='reference_deferred_index';
    lib.skill[existing]={trigger:{global:'useCardCancelled'},forced:true,silent:true,filter:e=>matches(e),async content(event,trigger){record('existing','useCardCancelled',trigger)}};
    lib.skill[late]={trigger:{global:'useCardCancelled'},forced:true,silent:true,filter:e=>matches(e),async content(event,trigger){record('late','useCardCancelled',trigger)}};
    lib.skill[indexed]={trigger:{global:'useCardCancelled'},forced:true,silent:true,
      getIndex(e){record('getIndex','useCardCancelled',e,indexCount);return indexCount},filter:e=>matches(e),
      async content(event,trigger){record('indexed','useCardCancelled',trigger,event.indexedData??null)}};
    lib.skill.reference_deferred_expire={charlotte:true,onremove(player){const cards=player.getExpansions('reference_deferred_expire');if(cards.length)player.loseToDiscardpile(cards)}};
    for(const name of [existing,late,indexed,'reference_deferred_expire'])game.finishSkill(name);
    const observer='reference_deferred_observer';
    lib.skill[observer]={trigger:{global:timings},firstDo:true,priority:15,forced:true,silent:true,filter:e=>matches(e),
      async content(event,trigger){record('observer',event.triggername,trigger)}};game.finishSkill(observer);E.addSkill(observer);
    const first='reference_deferred_cancel';
    lib.skill[first]={trigger:{global:'useCardToPlayered'},firstDo:true,priority:0,forced:true,silent:true,
      filter:e=>matches(e)&&e.player===A&&e.target===B,
      async content(event,trigger){record('before-cancel',event.triggername,trigger);let gain;
        if(trial==='fifo-gain-first'){gain=seats[2].gain(expansion,B,'give');record('gain-queued',event.triggername,trigger)}
        trigger.getParent('useCard',true).cancel();record('after-cancel',event.triggername,trigger);
        if(trial==='fifo-cancel-first'){gain=seats[2].gain(expansion,B,'give');record('gain-queued',event.triggername,trigger)}
        if(gain)await gain;}};
    game.finishSkill(first);E.addSkill(first);
    const last='reference_deferred_mutate';
    lib.skill[last]={trigger:{global:'useCardToPlayered'},lastDo:true,priority:0,forced:true,silent:true,
      filter:e=>matches(e)&&e.player===A&&e.target===B,
      async content(event,trigger){record('before-mutation',event.triggername,trigger);
        if(trial==='out-return'||trial==='index-out-return')D.in();
        if(trial==='new-listener')E.addSkill(late);
        if(trial==='index-change')indexCount=5;
        if(trial==='index-remove')D.removeSkill(indexed);
        if(trial==='index-readd'){D.removeSkill(indexed);D.addSkill(indexed)}
        record('after-mutation',event.triggername,trigger)}};
    game.finishSkill(last);E.addSkill(last);
    const proto=lib.element.GameEvent.prototype,original=proto.trigger;
    proto.trigger=function(timing){if(timings.includes(timing))record('request',timing,this);return original.apply(this,arguments)};
    try{
      for(const p of seats){const held=p.getCards('hej');if(held.length)await p.discard(held)}
      const sword=game.createCard2('qinggang','spade',6);await A.equip(sword);
      for(const name of ['out-return','new-listener','index-change','index-remove','index-readd','index-out-return','fifo-gain-first','fifo-cancel-first']){
        D.in();D.removeSkill([existing,indexed]);E.removeSkill(late);B.removeSkill(['qinggang2','reference_deferred_expire']);
        if(name==='out-return'){D.addSkill(existing);D.out()}
        if(name.startsWith('index-')){indexCount=2;D.addSkill(indexed);if(name==='index-out-return')D.out()}else indexCount=0;
        expansion=null; if(name.startsWith('fifo-')){expansion=game.createCard2('tao','heart',3);tracked.set(expansion,`${name}-expansion`);await B.gain(expansion);const added=B.addToExpansion(expansion,B,'give');added.gaintag.add('reference_deferred_expire');await added;}
        B.addTempSkill('reference_deferred_expire',{global:'useCardCancelled'});
        outer=game.createCard2('sha','spade',7);tracked.set(outer,name);await A.gain(outer);card=null;trial=name;
        await A.useCard(outer,B,false);stages.push([name,state()]);trial=null;
      }
      return{columns:['trial','kind','timing','frame','event','parentId','parent','player','target','detail','state','cards'],events,stages};
    }finally{proto.trigger=original;D.in();D.removeSkill([existing,indexed]);E.removeSkill([observer,first,last,late]);B.removeSkill(['qinggang2','reference_deferred_expire']);}
  };
}
export function checkCancelDeferred(result) {
  const trials=['out-return','new-listener','index-change','index-remove','index-readd','index-out-return'];
  const rows=trial=>result.events.filter(row=>row[0]===trial);
  const kind=(trial,name)=>rows(trial).filter(row=>row[1]===name);
  for(const trial of trials){
    const events=rows(trial), req=events.find(row=>row[1]==='request'&&row[2]==='useCardCancelled');
    const target=events.find(row=>row[1]==='request'&&row[2]==='useCardToPlayered');
    const before=kind(trial,'before-cancel')[0],after=kind(trial,'after-cancel')[0];
    assert.ok(req&&target&&before&&after,trial);
    assert.ok(events.indexOf(before)<events.indexOf(req)&&events.indexOf(req)<events.indexOf(after),trial);
    assert.equal(req[3],target[5]);assert.equal(req[4],'useCard');assert.equal(target[4],'useCardToPlayered');
    assert.equal(req[10].expired,false);assert.equal(after[10].expired,true);
    assert.equal(req[10].material,'o');assert.equal(result.stages.find(row=>row[0]===trial)[1].material,'d');
    assert.equal(kind(trial,'before-mutation')[0][10].contains,true);
    assert.ok(events.indexOf(kind(trial,'after-mutation')[0])<events.indexOf(kind(trial,'observer').find(row=>row[2]==='useCardCancelled')));
  }
  assert.equal(kind('out-return','request').find(row=>row[2]==='useCardCancelled')[10].out,true);
  assert.equal(kind('out-return','existing').length,1);
  assert.equal(kind('out-return','existing')[0][10].out,false);
  assert.equal(kind('new-listener','after-mutation')[0][10].late,true);
  assert.equal(kind('new-listener','late').length,0);
  for(const trial of ['index-change','index-remove','index-readd','index-out-return']){
    assert.deepEqual(kind(trial,'getIndex').map(row=>row[9]),[2],trial);
    assert.ok(rows(trial).indexOf(kind(trial,'getIndex')[0])<rows(trial).indexOf(kind(trial,'after-cancel')[0]));
  }
  assert.equal(kind('index-change','after-mutation')[0][10].index,5);
  assert.equal(kind('index-change','indexed').length,2);
  assert.equal(kind('index-remove','indexed').length,0);
  assert.equal(kind('index-readd','indexed').length,2);
  assert.equal(kind('index-out-return','getIndex')[0][10].out,true);
  assert.equal(kind('index-out-return','indexed').length,2);
  assert.ok(kind('index-out-return','indexed').every(row=>!row[10].out));
  for(const trial of ['fifo-gain-first','fifo-cancel-first']) {
    const events=rows(trial), at=(kind,timing)=>events.findIndex(row=>row[1]===kind&&row[2]===timing);
    const final=result.stages.find(row=>row[0]===trial)[1];
    assert.equal(final.expansion,'C:h');assert.equal(final.material,'d');
    assert.ok(at('request','useCardCancelled')<at('after-cancel','useCardToPlayered'));
    assert.ok(events.some(row=>row[1]==='observer'&&row[2]==='loseBefore'&&row[11].includes(`${trial}-expansion`)));
    assert.ok(events.some(row=>row[1]==='observer'&&row[2]==='gainAfter'&&row[11].includes(`${trial}-expansion`)));
    if(trial==='fifo-gain-first'){
      assert.ok(at('gain-queued','useCardToPlayered')<at('request','useCardCancelled'));
      assert.ok(at('request','gainAfter')<at('request','loseToDiscardpileBefore'));
      assert.equal(events[at('request','loseToDiscardpileBefore')][10].expansion,'C:h');
    }else{
      assert.ok(at('request','useCardCancelled')<at('gain-queued','useCardToPlayered'));
      assert.ok(at('request','loseToDiscardpileAfter')<at('request','gainBefore'));
      assert.equal(events[at('request','gainBefore')][10].expansion,'d');
    }
  }
}
