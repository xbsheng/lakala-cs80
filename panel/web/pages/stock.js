// 自定义页面示例：自选股 + 分时图（整屏 800×1280，不使用组件系统）
//
// 约定：export default { needs, draw(ctx, size, data, cfg, theme) }
//   needs: { kind: 'portfolio', ttl, query(cfg) } → 数据来自 host 的 /api/data/<kind>
//   draw : 想怎么画就怎么画，ctx 是 800×1280 的设备像素画布
//
// 数据源：host 侧优先 efinance / akshare（装了才用），否则走东财、新浪的公开 HTTP 接口。
const FONT = '-apple-system, BlinkMacSystemFont, "Inter", "PingFang SC", "Microsoft YaHei", sans-serif';
const MONO = '"SF Mono", "JetBrains Mono", ui-monospace, Menlo, monospace';

const UP = '#ef6f6f';
const DOWN = '#5fc98d';
const FG2 = '#b4b5bd';
const FG3 = '#8b8c96';
const FG4 = '#52535c';
const CARD = '#1a1c22';

import { minuteChart, stockRow, stockRangeTags, stockStamp, stockPolling, tradingHours, minuteSlots } from '../widgets.js';

const font = (size, weight = 400, mono = false) => `${weight} ${size}px ${mono ? MONO : FONT}`;

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  if (ctx.roundRect) ctx.roundRect(x, y, w, h, Math.min(r, w / 2, h / 2));
  else ctx.rect(x, y, w, h);
}

function ellipsis(ctx, text, maxW) {
  let s = String(text ?? '');
  if (ctx.measureText(s).width <= maxW) return s;
  while (s.length > 1 && ctx.measureText(s + '…').width > maxW) s = s.slice(0, -1);
  return s + '…';
}

function tracked(ctx, text, x, y, spacing) {
  for (const ch of [...String(text)]) {
    ctx.fillText(ch, x, y);
    x += ctx.measureText(ch).width + spacing;
  }
}

export default {
  needs: { kind: 'portfolio', ttl: 20000, when: tradingHours, query: (cfg) => ({ symbols: cfg.symbols || 'sh000001' }) },

  draw(ctx, size, data, cfg, theme) {
    const { w, h } = size;
    ctx.fillStyle = (theme && theme.bg) || '#141519';
    ctx.fillRect(0, 0, w, h);

    // 页头
    const pad = 36;
    ctx.textBaseline = 'top';
    ctx.textAlign = 'left';
    ctx.fillStyle = FG4;
    ctx.font = font(20, 600);
    tracked(ctx, (cfg.title || '自选股').toUpperCase(), pad, pad, 2.6);
    // 数据新鲜度：轮询中“5s前更新”，休盘/午休“10:28更新”，拉失败标黄
    const stamp = stockStamp(data, stockPolling());
    if (stamp) {
      ctx.textAlign = 'left';
      ctx.font = font(18, 400, true);
      ctx.fillStyle = stamp.color || FG4;
      ctx.fillText(stamp.text, pad, pad + 30);
    }
    ctx.textAlign = 'right';
    ctx.font = font(18, 400, true);
    ctx.fillText(
      new Date().toLocaleTimeString('zh-CN', { hour12: false }),
      w - pad,
      pad
    );

    const items = (data && data.items) || [];
    if (!items.length) {
      ctx.textAlign = 'left';
      ctx.fillStyle = FG4;
      ctx.font = font(22);
      ctx.fillText(data && data.error ? '数据源报错：' + data.error : '加载中…', pad, pad + 70);
      return;
    }

    // 第一支：大卡片（现价 + 分时）
    const lead = items[0];
    const cardY = pad + 56;
    const cardH = 420;
    roundRect(ctx, pad, cardY, w - pad * 2, cardH, 12);
    ctx.fillStyle = (theme && theme.card) || CARD;
    ctx.fill();
    ctx.strokeStyle = 'rgba(255,255,255,.06)';
    ctx.stroke();

    const up = (lead.pct ?? 0) >= 0;
    const color = up ? UP : DOWN;
    ctx.textAlign = 'left';
    ctx.fillStyle = FG3;
    ctx.font = font(24, 500);
    ctx.fillText(ellipsis(ctx, lead.name || lead.symbol, w * 0.5), pad + 28, cardY + 26);
    const priceTxt = Number(lead.price ?? 0).toFixed(2);
    const pctTxt = `${up ? '+' : ''}${Number(lead.pct ?? 0).toFixed(2)}%`;
    const priceY = cardY + 66;
    ctx.fillStyle = color; // 现价跟涨跌同色
    ctx.font = font(72, 300, true);
    const priceW = ctx.measureText(priceTxt).width;
    ctx.fillText(priceTxt, pad + 28, priceY);
    // 涨跌幅右对齐在现价同一视觉行上，避免和价格叠字
    ctx.fillStyle = color;
    ctx.font = font(30, 500, true);
    ctx.textAlign = 'right';
    ctx.fillText(pctTxt, w - pad - 28, priceY + 16);
    ctx.textAlign = 'left';
    ctx.fillStyle = FG4;
    ctx.font = font(19, 400, true);
    ctx.fillText(`昨收 ${Number(lead.preClose ?? lead.prev ?? 0).toFixed(2)}`, pad + 28, priceY + 92);

    const chart = { x: pad + 28, y: cardY + 186, w: w - pad * 2 - 56, h: cardH - 202 };
    minuteChart(
      ctx,
      chart,
      lead.points,
      lead.preClose ?? lead.prev,
      color,
      minuteSlots(lead.symbol)
    );
    stockRangeTags(ctx, chart, lead, theme || { cardHi: CARD, fg3: FG3 }, 15);

    // 其余标的：一行一只，和网格里的股票组件同一套画法
    const rest = items.slice(1);
    let y = cardY + cardH + 30;
    const rowH = rest.length ? Math.min(124, Math.max(78, (h - y - 96) / rest.length)) : 92;
    rest.slice(0, 7).forEach((it) => {
      stockRow(ctx, { x: pad, y, w: w - pad * 2, h: rowH - 12 }, it, {
        up: UP, down: DOWN, fg2: FG2, fg3: FG3, fg4: FG4, cardHi: CARD,
      });
      y += rowH;
    });

    // 页脚：数据源
    ctx.textAlign = 'left';
    ctx.fillStyle = FG4;
    ctx.font = font(16, 400, true);
    ctx.fillText(
      `data: ${(data && data.source) || '—'} · ${items.length} symbols`,
      pad,
      h - 48
    );
    if (data && data.items && data.items[0] && data.items[0].source) {
      ctx.textAlign = 'right';
      ctx.fillText(`quote: ${data.items[0].source}`, w - pad, h - 48);
    }
  },
};
