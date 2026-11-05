/** 节点坐标表：entry id → React Flow 坐标。 */
export type LayoutPositions = Record<string, { x: number; y: number }>;

/**
 * 把磁盘布局合并进内存坐标。
 *
 * - 磁盘上有的键优先（它就是后端迁移/落库后的真值）；
 * - 磁盘上没有的键保留内存值，避免刷新时把用户刚拖好的节点打回默认网格；
 * - `loaded` 为空/未加载时原样返回，调用方不必先判空。
 *
 * 这个函数同时被「entries 刷新后回读布局」和「拖拽落库后补偿对齐」使用，
 * 保证两处的合并语义一致：拖拽期间被跳过的布局变化，会在落库后一次性补回。
 */
export function mergeLayoutPositions(
  prev: LayoutPositions,
  loaded: LayoutPositions | null | undefined
): LayoutPositions {
  if (!loaded) return prev;
  return { ...prev, ...loaded };
}
