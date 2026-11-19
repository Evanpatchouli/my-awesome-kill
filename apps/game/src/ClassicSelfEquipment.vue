<script setup lang="ts">
import { computed } from 'vue'
import { slot } from '@mak/rules'
import { cardName, printedFace, type SeatView, type TableCard } from './classicProtocol'

const props = defineProps<{ seat: SeatView; equipment: TableCard[]; mountCombined?: boolean }>()

type EquipmentSlot = 'weapon' | 'armor' | 'defenseHorse' | 'attackHorse' | 'treasure'
const equipmentSlot = (kind: string): EquipmentSlot => {
  const type = slot(kind)
  if (type === 'plus') return 'defenseHorse'
  if (type === 'minus') return 'attackHorse'
  return type ?? 'treasure'
}

const rows = computed(() => {
  const slots: { key: EquipmentSlot; label: string }[] = [
    { key: 'weapon', label: '武器' },
    { key: 'armor', label: '防具' },
    { key: 'defenseHorse', label: props.mountCombined ? '坐骑' : '防御马' },
    ...(!props.mountCombined ? [{ key: 'attackHorse' as const, label: '进攻马' }] : []),
  ]
  const hasTreasure = props.equipment.some(card => equipmentSlot(card.kind) === 'treasure')
    || props.seat.detachedEquipment?.some(kind => equipmentSlot(kind) === 'treasure')
    || props.seat.equipmentDisabledMarkers?.includes('treasure')
  if (hasTreasure) slots.push({ key: 'treasure', label: '宝物' })
  return slots.map(item => {
    const matches = (key: EquipmentSlot) => key === item.key
      || (props.mountCombined && item.key === 'defenseHorse' && key === 'attackHorse')
    return {
      ...item,
      cards: props.equipment.filter(card => matches(equipmentSlot(card.kind))),
      detached: (props.seat.detachedEquipment ?? []).filter(kind => matches(equipmentSlot(kind))),
      disabled: (props.seat.equipmentDisabledMarkers ?? []).some(matches),
    }
  })
})
</script>

<template>
  <section class="classic-self-equipment" aria-label="自己的装备区">
    <div v-for="row in rows" :key="row.key" class="equipment-row" :class="{ disabled: row.disabled }">
      <span class="equipment-label">{{ row.label }}</span>
      <span class="equipment-content">
        <template v-if="row.cards.length">{{ row.cards.map(printedFace).join(' · ') }}</template>
        <template v-else>{{ row.disabled ? '已废除' : '空' }}</template>
        <template v-if="row.disabled && row.cards.length"> · 已废除</template>
        <template v-if="row.detached.length"> · 生效：{{ row.detached.map(kind => cardName(kind)).join(' · ') }}</template>
      </span>
    </div>
  </section>
</template>

<style scoped>
.classic-self-equipment { display: grid; grid-auto-rows: minmax(16px, 1fr); gap: 3px; min-width: 0; }
.equipment-row { display: flex; align-items: center; gap: 6px; min-width: 0; padding: 0 6px; border: 1px solid #80613a; border-radius: 3px; background: #30251d; color: #ead5ac; font-size: 12px; line-height: 1.15; }
.equipment-row.disabled { opacity: .65; }
.equipment-label { flex: none; color: #c8a975; }
.equipment-content { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
@media (max-height: 500px) and (orientation: landscape) {
  .equipment-row { gap: 4px; padding-inline: 4px; font-size: 10px; }
}
</style>
