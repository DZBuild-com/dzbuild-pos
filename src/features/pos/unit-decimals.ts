/** The decimals a quantity of `product` takes at the till, the same for a scan, a search result
 * and a tile: the product's own when it carries them (a scanned product does), else its unit's
 * from the units list, else pieces are whole and weights, volumes and lengths go to the
 * thousandth. */
export function unitDecimalsOf(
  product: { unitCode: string; unitDecimals?: number },
  units?: readonly { code: string; decimals: number }[],
): number {
  if (product.unitDecimals !== undefined) return product.unitDecimals;
  const unit = units?.find((u) => u.code === product.unitCode);
  if (unit) return unit.decimals;
  return product.unitCode === "pc" ? 0 : 3;
}
