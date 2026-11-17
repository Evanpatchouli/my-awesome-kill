export const heroes = {
  zhangfei: {
    name: '张飞',
    hp: 4,
    sex: 'male',
    skills: ['咆哮：出牌阶段使用杀没有次数限制。'],
  },
  huangyueying: {
    name: '黄月英',
    hp: 3,
    sex: 'female',
    skills: ['集智：使用普通锦囊时，可以摸一张牌。', '奇才：使用锦囊牌没有距离限制。'],
  },
} as const
export type Hero = keyof typeof heroes
