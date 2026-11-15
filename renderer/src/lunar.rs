// 公历 → 农历，移植自 panel/web/lunar.js（1900–2100 位表）。
const LUNAR_INFO: [u32; 201] = [
    0x04bd8, 0x04ae0, 0x0a570, 0x054d5, 0x0d260, 0x0d950, 0x16554, 0x056a0, 0x09ad0, 0x055d2,
    0x04ae0, 0x0a5b6, 0x0a4d0, 0x0d250, 0x1d255, 0x0b540, 0x0d6a0, 0x0ada2, 0x095b0, 0x14977,
    0x04970, 0x0a4b0, 0x0b4b5, 0x06a50, 0x06d40, 0x1ab54, 0x02b60, 0x09570, 0x052f2, 0x04970,
    0x06566, 0x0d4a0, 0x0ea50, 0x06e95, 0x05ad0, 0x02b60, 0x186e3, 0x092e0, 0x1c8d7, 0x0c950,
    0x0d4a0, 0x1d8a6, 0x0b550, 0x056a0, 0x1a5b4, 0x025d0, 0x092d0, 0x0d2b2, 0x0a950, 0x0b557,
    0x06ca0, 0x0b550, 0x15355, 0x04da0, 0x0a5b0, 0x14573, 0x052b0, 0x0a9a8, 0x0e950, 0x06aa0,
    0x0aea6, 0x0ab50, 0x04b60, 0x0aae4, 0x0a570, 0x05260, 0x0f263, 0x0d950, 0x05b57, 0x056a0,
    0x096d0, 0x04dd5, 0x04ad0, 0x0a4d0, 0x0d4d4, 0x0d250, 0x0d558, 0x0b540, 0x0b6a0, 0x195a6,
    0x095b0, 0x049b0, 0x0a974, 0x0a4b0, 0x0b27a, 0x06a50, 0x06d40, 0x0af46, 0x0ab60, 0x09570,
    0x04af5, 0x04970, 0x064b0, 0x074a3, 0x0ea50, 0x06b58, 0x05ac0, 0x0ab60, 0x096d5, 0x092e0,
    0x0c960, 0x0d954, 0x0d4a0, 0x0da50, 0x07552, 0x056a0, 0x0abb7, 0x025d0, 0x092d0, 0x0cab5,
    0x0a950, 0x0b4a0, 0x0baa4, 0x0ad50, 0x055d9, 0x04ba0, 0x0a5b0, 0x15176, 0x052b0, 0x0a930,
    0x07954, 0x06aa0, 0x0ad50, 0x05b52, 0x04b60, 0x0a6e6, 0x0a4e0, 0x0d260, 0x0ea65, 0x0d530,
    0x05aa0, 0x076a3, 0x096d0, 0x04afb, 0x04ad0, 0x0a4d0, 0x1d0b6, 0x0d250, 0x0d520, 0x0dd45,
    0x0b5a0, 0x056d0, 0x055b2, 0x049b0, 0x0a577, 0x0a4b0, 0x0aa50, 0x1b255, 0x06d20, 0x0ada0,
    0x14b63, 0x09370, 0x049f8, 0x04970, 0x064b0, 0x168a6, 0x0ea50, 0x06b20, 0x1a6c4, 0x0aae0,
    0x092e0, 0x0d2e3, 0x0c960, 0x0d557, 0x0d4a0, 0x0da50, 0x05d55, 0x056a0, 0x0a6d0, 0x055d4,
    0x052d0, 0x0a9b8, 0x0a950, 0x0b4a0, 0x0b6a6, 0x0ad50, 0x055a0, 0x0aba4, 0x0a5b0, 0x052b0,
    0x0b273, 0x06930, 0x07337, 0x06aa0, 0x0ad50, 0x14b55, 0x04b60, 0x0a570, 0x054e4, 0x0d160,
    0x0e968, 0x0d520, 0x0daa0, 0x16aa6, 0x056d0, 0x04ae0, 0x0a9d4, 0x0a4d0, 0x0d150, 0x0f252,
    0x0d520,
];
const MONTH_NAMES: [&str; 12] = ["正","二","三","四","五","六","七","八","九","十","冬","腊"];
const DAY_TENS: [&str; 4] = ["初","十","廿","卅"];
const DAY_UNITS: [&str; 10] = ["十","一","二","三","四","五","六","七","八","九"];

fn leap_month(y: i32) -> u32 { LUNAR_INFO[(y - 1900) as usize] & 0xf }
fn leap_days(y: i32) -> i32 {
    if leap_month(y) != 0 { if LUNAR_INFO[(y - 1900) as usize] & 0x10000 != 0 { 30 } else { 29 } } else { 0 }
}
fn month_days(y: i32, m: i32) -> i32 {
    if LUNAR_INFO[(y - 1900) as usize] & (0x10000 >> m) != 0 { 30 } else { 29 }
}
fn year_days(y: i32) -> i32 {
    let mut sum = 348;
    let mut i = 0x8000;
    while i > 0x8 {
        if LUNAR_INFO[(y - 1900) as usize] & i != 0 { sum += 1; }
        i >>= 1;
    }
    sum + leap_days(y)
}
fn day_name(d: i32) -> String {
    if d == 10 { return "初十".into(); }
    if d == 20 { return "二十".into(); }
    if d == 30 { return "三十".into(); }
    format!("{}{}", DAY_TENS[(d / 10) as usize], DAY_UNITS[(d % 10) as usize])
}

pub fn solar_to_lunar(year: i32, month: i32, day: i32) -> String {
    use chrono::NaiveDate;
    let base = NaiveDate::from_ymd_opt(1900, 1, 31).unwrap();
    let date = NaiveDate::from_ymd_opt(year, month as u32, day as u32).unwrap();
    let mut offset = (date - base).num_days() as i32;
    let mut i = 1900;
    let mut temp = 0;
    while i < 2101 && offset > 0 {
        temp = year_days(i);
        offset -= temp;
        i += 1;
    }
    if offset < 0 { offset += temp; i -= 1; }
    let lyear = i;
    let leap = leap_month(lyear) as i32;
    let mut is_leap = false;
    i = 1;
    while i < 13 && offset > 0 {
        if leap > 0 && i == leap + 1 && !is_leap {
            i -= 1;
            is_leap = true;
            temp = leap_days(lyear);
        } else {
            temp = month_days(lyear, i);
        }
        if is_leap && i == leap + 1 { is_leap = false; }
        offset -= temp;
        i += 1;
    }
    if offset == 0 && leap > 0 && i == leap + 1 {
        if is_leap { is_leap = false; } else { is_leap = true; i -= 1; }
    }
    if offset < 0 { offset += temp; i -= 1; }
    let lmonth = i;
    let lday = offset + 1;
    let mn = MONTH_NAMES[(lmonth - 1) as usize];
    format!("{}{}月{}", if is_leap { "闰" } else { "" }, mn, day_name(lday))
}
