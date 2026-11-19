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
    const cards = props.equipment.filter(card => matches(equipmentSlot(card.kind)))
    const detached = (props.seat.detachedEquipment ?? []).filter(kind => matches(equipmentSlot(kind)))
    const disabled = (props.seat.equipmentDisabledMarkers ?? []).some(matches)
    const text = cards.map(printedFace)
    if (disabled) text.push('已废除')
    if (detached.length) text.push(`生效：${detached.map(kind => cardName(kind)).join(' · ')}`)
    return { ...item, empty: !cards.length && !detached.length, disabled, detail: text.join(' · ') }
  })
})
</script>

<template>
  <section class="classic-self-equipment" aria-label="自己的装备区">
    <div
      v-for="row in rows"
      :key="row.key"
      class="equipment-row"
      :class="{ disabled: row.disabled }"
      :title="row.detail || row.label"
    >
      <span v-if="row.empty" class="equipment-label">{{ row.label }}</span>
      <span v-if="row.detail" class="equipment-content">{{ row.detail }}</span>
    </div>
  </section>
</template>

<style scoped>
.classic-self-equipment { display: grid; grid-auto-rows: minmax(16px, 1fr); gap: 3px; min-width: 0; }
.equipment-row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  padding: 0 clamp(6px, .65vw, 12px);
  border-radius: 3px;
  background: #252521 url('./assets/battle/equipment-slot.webp') center / 100% 100% no-repeat;
  color: #f3dfbf;
  font-size: clamp(11px, 1vw, 18px);
  line-height: 1.15;
}
.equipment-row.disabled { opacity: .65; }
.equipment-label { flex: none; color: #aa9c80; }
.equipment-content { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
@media (max-height: 500px) and (orientation: landscape) {
  .equipment-row { gap: 4px; padding-inline: 4px; font-size: 10px; }
}
</style>
