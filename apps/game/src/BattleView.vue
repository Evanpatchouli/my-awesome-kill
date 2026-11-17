<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { gsap } from 'gsap'
import { useGame } from './game'
import { heroes } from '@mak/rules'
import { heroPortrait } from './battleArt'
import CardFace from './CardFace.vue'
import SeatZones from './SeatZones.vue'
import HealthMeter from './HealthMeter.vue'
import BattleEffects from './BattleEffects.vue'

const emit = defineEmits<{ exit: []; restart: [] }>()
const game = useGame()
const battleScreen = ref<HTMLElement | null>(null)
const discardSelection = ref<number[]>([])
const suits = { spade: '♠', heart: '♥', club: '♣', diamond: '♦' }
const selected = computed(() =>
  game.view?.players[0].hand.find((card) => card.id === game.selectedCard),
)
const selectedChoices = computed(
  () => game.view?.choices.filter((choice) => choice.card === game.selectedCard) ?? [],
)
const otherChoices = computed(
  () =>
    game.view?.choices.filter(
      (choice) =>
        choice.card === undefined ||
        !game.view?.players[0].hand.some((card) => card.id === choice.card),
    ) ?? [],
)
function isSecondary(label: string) {
  return /结束|放弃|不响应|不使用|不出|保留|取消|跳过|不发动/.test(label)
}
function choiceHandler(key: string, revision: number) {
  return () => game.act(key, revision)
}
function cardHandler(id: number, revision: number) {
  return () => {
    if (revision !== game.view?.revision || game.locked || game.waiting || game.view.actor !== 0) return
    if (game.view.discardCountRequired) {
      if (discardSelection.value.includes(id)) discardSelection.value = discardSelection.value.filter((value) => value !== id)
      else if (discardSelection.value.length < game.view.discardCountRequired) discardSelection.value = [...discardSelection.value, id]
    } else game.selectedCard = id
  }
}
watch(
  () => game.view,
  () => {
    discardSelection.value = []
    if (!battleScreen.value || localStorage.getItem('mak-reduced-motion') === 'true') return
    const prompt = battleScreen.value.querySelector('.battle-prompt')
    if (prompt) {
      gsap.killTweensOf(prompt)
      gsap.fromTo(prompt, { opacity: 0.55, y: 5 }, { opacity: 1, y: 0, duration: 0.25 })
    }
  },
)
onUnmounted(() => {
  if (battleScreen.value) gsap.killTweensOf(battleScreen.value.querySelectorAll('*'))
})
</script>
<template>
  <section v-if="game.view" ref="battleScreen" class="battle-page" :data-revision="game.view.revision">
    <BattleEffects :view="game.view" />
    <div class="battle-layout grid gap-8" data-battle-table>
      <aside class="combatant opponent" data-seat="1">
        <div class="portrait">
          <img :src="heroPortrait(game.view.players[1].hero)" :alt="game.view.players[1].name" /><span
            >对手</span
          >
        </div>
        <div>
          <h2>{{ game.view.players[1].name }}</h2>
          <HealthMeter :hp="game.view.players[1].hp" :max-hp="game.view.players[1].maxHp" />
          <p>手牌 {{ game.view.players[1].handCount }}</p>

        </div>
      </aside>
      <SeatZones class="opponent-zone" label="对手" :equipment="game.view.players[1].equipment" :judges="game.view.players[1].judges" />
      <div class="battle-center">
        <div class="turn-indicator">
          <span>{{
            game.view.winner !== null
              ? '对局结束'
              : game.view.active === 0
                ? '你的回合'
                : '对手的回合'
          }}</span
          ><small v-if="game.view.winner === null"
            >{{ game.view.phase }} · {{ game.view.actor === 0 ? '由你决策' : '对手决策' }}</small
          >
        </div>
        <div v-if="game.view.winner === null" class="battle-prompt">
          <span class="prompt-mark">令</span>
          <h1>{{ game.view.prompt }}</h1>
          <p v-if="game.view.log.length" class="latest-log">
            {{ game.view.log[game.view.log.length - 1] }}
          </p>
          <p v-if="game.waiting">对手正在思考<span class="thinking">···</span></p>
          <p v-else-if="game.view.actor === 0 && game.view.winner === null">
            选择手牌或下方操作继续
          </p>
        </div>
        <div v-if="game.view.revealed.length && game.view.winner === null" class="revealed-cards">
          公开牌：<span v-for="card in game.view.revealed" :key="card.id"
            >{{ suits[card.suit] }}{{ card.rank }} {{ card.name }}</span
          >
        </div>
        <p v-if="game.saveError" class="save-error">
          {{ game.pendingRecords.length }} 条战绩待保存。<button @click="game.retrySave()">
            重试保存
          </button>
        </p>
        <div v-if="game.view.winner !== null" class="result">
          <h2>{{ game.view.winner === 0 ? '胜' : '败' }}</h2>
          <p>{{ game.view.winner === 0 ? '此局，阁下更胜一筹。' : '胜败乃兵家常事。' }}</p>
          <button class="primary-button" :disabled="game.locked" @click="emit('restart')">
            再来一局 →
          </button>
        </div>
        <div v-else-if="game.view.discardCountRequired" class="action-row discard-actions">
          <span>已选 {{ discardSelection.length }} / {{ game.view.discardCountRequired }}</span>
          <button :disabled="game.locked || game.waiting || discardSelection.length !== game.view.discardCountRequired" @click="game.discard(discardSelection, game.view.revision)">确认弃牌</button>
        </div>
        <div v-else class="action-row">
          <button
            v-for="choice in otherChoices"
            :key="choice.key"
            :disabled="game.locked || game.waiting"
            :class="{ secondary: isSecondary(choice.label) }"
            v-on="{ click: choiceHandler(choice.key, game.view.revision) }"
          >
            {{ choice.label }}</button
          ><button
            v-for="choice in selectedChoices"
            :key="choice.key"
            :disabled="game.locked || game.waiting"
            :class="{ secondary: isSecondary(choice.label) }"
            v-on="{ click: choiceHandler(choice.key, game.view.revision) }"
          >
            {{ choice.label }}
          </button>
        </div>
      </div>
      <details class="battle-log">
        <summary>对局记录</summary>
        <ol>
          <li v-for="(line, index) in [...game.view.log].reverse().slice(0, 8)" :key="index">
            {{ line }}
          </li>
        </ol>
      </details>
    </div>
    <div class="player-area flex gap-8">
      <div class="player-meta" data-seat="0">
        <img
          class="player-portrait"
          :src="heroPortrait(game.view.players[0].hero)"
          :alt="game.view.players[0].name + '立绘'"
        />
        <div>
          <span>你</span>
          <h2>{{ game.view.players[0].name }}</h2>
          <HealthMeter :hp="game.view.players[0].hp" :max-hp="game.view.players[0].maxHp" />
        </div>
      </div>
      <SeatZones class="player-zone" label="你" :equipment="game.view.players[0].equipment" :judges="game.view.players[0].judges" />
      <div class="hand-zone">
        <details class="rules-detail">
          <summary>查看武将技能与场上牌说明</summary>
          <p>
            {{ game.view.players[0].name }}：{{ heroes[game.view.players[0].hero].skills.join(' ') }}
          </p>
          <p>
            {{ game.view.players[1].name }}：{{ heroes[game.view.players[1].hero].skills.join(' ') }}
          </p>
          <p
            v-for="card in [
              ...game.view.players[0].equipment,
              ...game.view.players[0].judges,
              ...game.view.players[1].equipment,
              ...game.view.players[1].judges,
            ]"
            :key="card.id"
          >
            {{ card.name }}：{{ card.description }}
          </p>
        </details>
        <div class="hand-heading">
          <span
            >手牌 <b>{{ game.view.players[0].handCount }}</b></span
          ><span>{{
            game.view.discardCountRequired ? `请选择 ${game.view.discardCountRequired} 张手牌弃置` : selected ? selected.description : '点击手牌查看牌效；可用手牌以边框标出'
          }}</span>
        </div>
        <div class="hand" data-hand>
          <button
            v-for="card in game.view.players[0].hand"
            :key="card.id"
            :disabled="game.locked || game.waiting || game.view.winner !== null || game.view.actor !== 0"
            class="playing-card"
            :class="{
              red: card.suit === 'heart' || card.suit === 'diamond',
              'long-name': card.name.length > 2,
              selected: game.view.discardCountRequired ? discardSelection.includes(card.id) : game.selectedCard === card.id,
              playable: game.view.choices.some((choice) => choice.card === card.id),
            }"
            :title="card.description"
            :aria-label="`${suits[card.suit]}${card.rank} ${card.name}：${card.description}`"
            :aria-pressed="game.view.discardCountRequired ? discardSelection.includes(card.id) : game.selectedCard === card.id"
            v-on="{ click: cardHandler(card.id, game.view.revision) }"
          >
            <CardFace :card="card" />
          </button>
        </div>
      </div>
    </div>
  </section>
</template>
