export type Suit = 'spade' | 'club' | 'heart' | 'diamond'
export type Slot = 'weapon' | 'armor' | 'plus' | 'minus'
export interface Card {
  id: number
  kind: string
  name: string
  suit: Suit
  rank: number
  description: string
}

interface Definition {
  name: string
  description: string
  faces: Partial<Record<Suit, number[]>>
  slot?: Slot
  range?: number
}

// The 104 physical cards in the classic standard set. Repeated ranks are distinct copies.
export const definitions: Readonly<Record<string, Definition>> = {
  sha: {
    name: '杀',
    description: '出牌阶段对攻击范围内的对手使用。对方须出闪，否则受到一点伤害。',
    faces: {
      spade: [7, 8, 8, 9, 9, 10, 10],
      club: [2, 3, 4, 5, 6, 7, 8, 8, 9, 9, 10, 10, 11, 11],
      heart: [10, 10, 11],
      diamond: [6, 7, 8, 9, 10, 13],
    },
  },
  shan: {
    name: '闪',
    description: '响应杀或万箭齐发，避免其伤害。',
    faces: { heart: [2, 2, 13], diamond: [2, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 11] },
  },
  tao: {
    name: '桃',
    description: '出牌时回复自己一点体力，或在濒死时救援一名角色。',
    faces: { heart: [3, 4, 6, 7, 8, 9, 12], diamond: [12] },
  },
  bagua: {
    name: '八卦阵',
    description: '需要闪时可判定。红色视为闪，黑色仍可用手牌闪。',
    slot: 'armor',
    faces: { spade: [2], club: [2] },
  },
  jueying: {
    name: '绝影',
    description: '对方计算与你的距离加一。',
    slot: 'plus',
    faces: { spade: [5] },
  },
  dilu: { name: '的卢', description: '对方计算与你的距离加一。', slot: 'plus', faces: { club: [5] } },
  zhuahuang: {
    name: '爪黄飞电',
    description: '对方计算与你的距离加一。',
    slot: 'plus',
    faces: { heart: [13] },
  },
  chitu: { name: '赤兔', description: '你计算与对方的距离减一。', slot: 'minus', faces: { heart: [5] } },
  dawan: {
    name: '大宛',
    description: '你计算与对方的距离减一。',
    slot: 'minus',
    faces: { spade: [13] },
  },
  zixin: {
    name: '紫骍',
    description: '你计算与对方的距离减一。',
    slot: 'minus',
    faces: { diamond: [13] },
  },
  zhuge: {
    name: '诸葛连弩',
    description: '出牌阶段使用杀没有次数限制。',
    slot: 'weapon',
    range: 1,
    faces: { club: [1], diamond: [1] },
  },
  cixiong: {
    name: '雌雄双股剑',
    description: '对异性使用杀，可令对手弃一张手牌或让你摸一张牌。',
    slot: 'weapon',
    range: 2,
    faces: { spade: [2] },
  },
  qinggang: {
    name: '青釭剑',
    description: '此杀结算时忽略目标的防具。',
    slot: 'weapon',
    range: 2,
    faces: { spade: [6] },
  },
  qinglong: {
    name: '青龙偃月刀',
    description: '杀被闪抵消后，可对同一目标继续使用杀。',
    slot: 'weapon',
    range: 3,
    faces: { spade: [5] },
  },
  zhangba: {
    name: '丈八蛇矛',
    description: '可将两张手牌当作一张杀使用或打出。',
    slot: 'weapon',
    range: 3,
    faces: { spade: [12] },
  },
  guanshi: {
    name: '贯石斧',
    description: '杀被闪抵消后，可弃置另外两张牌令此杀造成伤害。',
    slot: 'weapon',
    range: 3,
    faces: { diamond: [5] },
  },
  fangtian: {
    name: '方天画戟',
    description: '最后一张手牌作为杀时可额外指定至多两名目标。双人对局只有一名对手。',
    slot: 'weapon',
    range: 4,
    faces: { diamond: [12] },
  },
  qilin: {
    name: '麒麟弓',
    description: '杀造成伤害时可弃置对手装备的一张坐骑。',
    slot: 'weapon',
    range: 5,
    faces: { heart: [5] },
  },
  wugu: {
    name: '五谷丰登',
    description: '亮出两张牌，从使用者起双方依次选取一张。',
    faces: { heart: [3, 4] },
  },
  taoyuan: { name: '桃园结义', description: '双方依次回复一点体力。', faces: { heart: [1] } },
  nanman: {
    name: '南蛮入侵',
    description: '对方须打出杀，否则受到一点伤害。',
    faces: { spade: [7, 13], club: [7] },
  },
  wanjian: { name: '万箭齐发', description: '对方须打出闪，否则受到一点伤害。', faces: { heart: [1] } },
  juedou: {
    name: '决斗',
    description: '从对方起轮流打出杀，先未打出的一方受到一点伤害。',
    faces: { spade: [1], club: [1], diamond: [1] },
  },
  wuzhong: { name: '无中生有', description: '摸两张牌。', faces: { heart: [7, 8, 9, 11] } },
  shunshou: {
    name: '顺手牵羊',
    description: '获得距离一以内的对方区域中的一张牌。',
    faces: { spade: [3, 4, 11], diamond: [3, 4] },
  },
  guohe: {
    name: '过河拆桥',
    description: '弃置对方区域中的一张牌。',
    faces: { spade: [3, 4, 12], club: [3, 4], heart: [12] },
  },
  jiedao: {
    name: '借刀杀人',
    description: '持武器的对方须对你使用杀，否则将武器交给你。',
    faces: { club: [12, 13] },
  },
  wuxie: {
    name: '无懈可击',
    description: '抵消锦囊对一个目标的效果，也可抵消另一张无懈可击。',
    faces: { spade: [11], club: [12, 13] },
  },
  lebu: {
    name: '乐不思蜀',
    description: '置入对方判定区。判定非红桃则跳过出牌阶段。',
    faces: { spade: [6], club: [6], heart: [6] },
  },
  shandian: {
    name: '闪电',
    description: '置入自己判定区。黑桃二至九造成三点雷电伤害，否则传向下家。',
    faces: { spade: [1] },
  },
}

export function slot(kind: string) {
  return definitions[kind]?.slot
}

export function standardDeck(): Card[] {
  const cards: Card[] = []
  for (const [kind, def] of Object.entries(definitions)) {
    for (const [suit, ranks] of Object.entries(def.faces)) {
      for (const rank of ranks)
        cards.push({
          id: cards.length,
          kind,
          name: def.name,
          description: def.range ? '攻击范围 ' + def.range + '。' + def.description : def.description,
          suit: suit as Suit,
          rank,
        })
    }
  }
  return cards
}

export function face(c: Card) {
  const suits = { spade: '♠', heart: '♥', club: '♣', diamond: '♦' }
  return `${suits[c.suit]}${['A', '2', '3', '4', '5', '6', '7', '8', '9', '10', 'J', 'Q', 'K'][c.rank - 1]} ${c.name}`
}
