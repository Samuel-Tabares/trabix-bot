use crate::db::models::OrderItemData;

const LIQUOR_DETAIL_FULL_PRICE: u32 = 8_000;
const LIQUOR_DETAIL_PROMO_PRICE: u32 = 4_000;
const NON_LIQUOR_DETAIL_PRICE: u32 = 7_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedOrderItem {
    pub flavor: String,
    pub has_liquor: bool,
    pub quantity: u32,
    pub unit_price: u32,
    pub subtotal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemCalculated {
    pub flavor: String,
    pub has_liquor: bool,
    pub quantity: u32,
    pub subtotal: u32,
    pub is_wholesale: bool,
    pub promo_units: u32,
    pub unit_price_reference: Option<u32>,
    pub persistence_lines: Vec<PersistedOrderItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoCalculado {
    pub items_detalle: Vec<ItemCalculated>,
    pub total_con_licor: u32,
    pub total_sin_licor: u32,
    pub es_mayor_con_licor: bool,
    pub es_mayor_sin_licor: bool,
    pub total_estimado: u32,
}

pub fn calcular_precio_licor_detal(cantidad: u32) -> u32 {
    let pares = cantidad / 2;
    let impares = cantidad % 2;
    (pares * 12_000) + (impares * 8_000)
}

pub fn calcular_precio_sin_licor_detal(cantidad: u32) -> u32 {
    cantidad * NON_LIQUOR_DETAIL_PRICE
}

pub fn precio_unitario_mayor(cantidad: u32, has_liquor: bool) -> u32 {
    match (has_liquor, cantidad) {
        (true, 20..=49) => 4_900,
        (true, 50..=99) => 4_700,
        (true, 100..) => 4_500,
        (false, 20..=49) => 4_800,
        (false, 50..=99) => 4_500,
        (false, 100..) => 4_200,
        _ => unreachable!("solo se llama con cantidad >= 20"),
    }
}

pub fn calcular_pedido(items: &[OrderItemData]) -> PedidoCalculado {
    let total_con_licor_qty = total_quantity(items, true);
    let total_sin_licor_qty = total_quantity(items, false);
    let es_mayor_con_licor = total_con_licor_qty >= 20;
    let es_mayor_sin_licor = total_sin_licor_qty >= 20;

    let mut licor_posicion = 0;
    let mut items_detalle = Vec::with_capacity(items.len());
    let mut total_con_licor = 0;
    let mut total_sin_licor = 0;

    for item in items {
        let calculado = if item.has_liquor {
            if es_mayor_con_licor {
                calcular_item_mayor(item, precio_unitario_mayor(total_con_licor_qty, true))
            } else {
                let item_calculado = calcular_item_licor_detal(item, licor_posicion);
                licor_posicion += item.quantity;
                item_calculado
            }
        } else if es_mayor_sin_licor {
            calcular_item_mayor(item, precio_unitario_mayor(total_sin_licor_qty, false))
        } else {
            calcular_item_sin_licor_detal(item)
        };

        if item.has_liquor {
            total_con_licor += calculado.subtotal;
        } else {
            total_sin_licor += calculado.subtotal;
        }

        items_detalle.push(calculado);
    }

    PedidoCalculado {
        items_detalle,
        total_con_licor,
        total_sin_licor,
        es_mayor_con_licor,
        es_mayor_sin_licor,
        total_estimado: total_con_licor + total_sin_licor,
    }
}

fn total_quantity(items: &[OrderItemData], has_liquor: bool) -> u32 {
    items
        .iter()
        .filter(|item| item.has_liquor == has_liquor)
        .map(|item| item.quantity)
        .sum()
}

fn calcular_item_mayor(item: &OrderItemData, unit_price: u32) -> ItemCalculated {
    let subtotal = item.quantity * unit_price;

    ItemCalculated {
        flavor: item.flavor.clone(),
        has_liquor: item.has_liquor,
        quantity: item.quantity,
        subtotal,
        is_wholesale: true,
        promo_units: 0,
        unit_price_reference: Some(unit_price),
        persistence_lines: vec![PersistedOrderItem {
            flavor: item.flavor.clone(),
            has_liquor: item.has_liquor,
            quantity: item.quantity,
            unit_price,
            subtotal,
        }],
    }
}

fn calcular_item_sin_licor_detal(item: &OrderItemData) -> ItemCalculated {
    let subtotal = calcular_precio_sin_licor_detal(item.quantity);

    ItemCalculated {
        flavor: item.flavor.clone(),
        has_liquor: false,
        quantity: item.quantity,
        subtotal,
        is_wholesale: false,
        promo_units: 0,
        unit_price_reference: Some(NON_LIQUOR_DETAIL_PRICE),
        persistence_lines: vec![PersistedOrderItem {
            flavor: item.flavor.clone(),
            has_liquor: false,
            quantity: item.quantity,
            unit_price: NON_LIQUOR_DETAIL_PRICE,
            subtotal,
        }],
    }
}

fn calcular_item_licor_detal(item: &OrderItemData, start_position: u32) -> ItemCalculated {
    let mut subtotal = 0;
    let mut regular_units = 0;
    let mut promo_units = 0;

    for offset in 0..item.quantity {
        let is_promo = (start_position + offset + 1) % 2 == 0;
        if is_promo {
            subtotal += LIQUOR_DETAIL_PROMO_PRICE;
            promo_units += 1;
        } else {
            subtotal += LIQUOR_DETAIL_FULL_PRICE;
            regular_units += 1;
        }
    }

    let mut persistence_lines = Vec::with_capacity(2);
    if regular_units > 0 {
        persistence_lines.push(PersistedOrderItem {
            flavor: item.flavor.clone(),
            has_liquor: true,
            quantity: regular_units,
            unit_price: LIQUOR_DETAIL_FULL_PRICE,
            subtotal: regular_units * LIQUOR_DETAIL_FULL_PRICE,
        });
    }
    if promo_units > 0 {
        persistence_lines.push(PersistedOrderItem {
            flavor: item.flavor.clone(),
            has_liquor: true,
            quantity: promo_units,
            unit_price: LIQUOR_DETAIL_PROMO_PRICE,
            subtotal: promo_units * LIQUOR_DETAIL_PROMO_PRICE,
        });
    }

    ItemCalculated {
        flavor: item.flavor.clone(),
        has_liquor: true,
        quantity: item.quantity,
        subtotal,
        is_wholesale: false,
        promo_units,
        unit_price_reference: None,
        persistence_lines,
    }
}
