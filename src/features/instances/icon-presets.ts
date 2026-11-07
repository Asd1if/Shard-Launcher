/**
 * Встроенные иконки сборок — свои пиксельные рисунки 8×8.
 * Каждая строка — ряд пикселей, символ — цвет из палитры иконки, точка — прозрачно.
 */
export interface IconPreset {
  id: string;
  /** Фон плитки под рисунком */
  background: string;
  palette: Record<string, string>;
  rows: string[];
}

export const iconPresets: IconPreset[] = [
  {
    id: "grass",
    background: "#2a3a22",
    palette: { g: "#6aa84f", G: "#8bc34a", d: "#7a5230", D: "#5c3b20" },
    rows: ["GgGgGGgG", "gGgGgGGg", "gdgGdgdG", "dDddDdDd", "dddDddDd", "DddddDdd", "ddDddddD", "dDddDddd"],
  },
  {
    id: "stone",
    background: "#26282c",
    palette: { s: "#8a8d91", S: "#a5a8ac", k: "#6b6e72" },
    rows: ["SsSkssSs", "sskSsSks", "kSsssSsS", "sSsksskS", "SsksSssk", "ssSsksSs", "kssSsSks", "sSksssSs"],
  },
  {
    id: "chest",
    background: "#33261a",
    palette: { w: "#b07a3c", W: "#d29a52", b: "#5a3a1a", l: "#e0c060" },
    rows: ["bbbbbbbb", "bWWWWWWb", "bwwwwwwb", "bbbllbbb", "bWWllWWb", "bwwwwwwb", "bwwwwwwb", "bbbbbbbb"],
  },
  {
    id: "sword",
    background: "#22303a",
    palette: { s: "#d8e4ec", S: "#9fb3c0", h: "#7a5230", g: "#e0b040" },
    rows: ["......sS", ".....sS.", "....sS..", "...sS...", "g.sS....", ".gS.....", ".hg.....", "h..g...."],
  },
  {
    id: "pickaxe",
    background: "#24303a",
    palette: { p: "#5ad1c8", P: "#38a59c", h: "#7a5230", H: "#5c3b20" },
    rows: [".PPPPP..", "P....hP.", ".....hP.", "....h..P", "...h....", "..H.....", ".H......", "H......."],
  },
  {
    id: "gem",
    background: "#1f2f38",
    palette: { a: "#9ff3ff", b: "#4fd6e8", c: "#2a9fb4", d: "#1b6f80" },
    rows: ["..aaaa..", ".abbbba.", "abbccbba", "bbccccbb", ".dccccd.", "..dccd..", "...dd...", "........"],
  },
  {
    id: "apple",
    background: "#3a2222",
    palette: { r: "#d83a3a", R: "#f06060", d: "#9a2020", s: "#5c3b20", l: "#6aa84f" },
    rows: ["....sl..", "...s.ll.", ".rrRrrr.", "rrRRrrrr", "rrRrrrrr", "rrrrrrrd", ".rrrrrd.", "..dd.d.."],
  },
  {
    id: "heart",
    background: "#38202a",
    palette: { r: "#e04050", R: "#ff8090", d: "#a02030" },
    rows: [".rr..rr.", "rRrrrrrr", "rRrrrrrr", "rrrrrrrd", ".rrrrrd.", "..rrrd..", "...rd...", "........"],
  },
  {
    id: "flame",
    background: "#3a2a18",
    palette: { y: "#ffe070", o: "#ffa030", r: "#e05020" },
    rows: ["...r....", "...rr...", "..rrr.r.", "..rorrr.", ".rroorr.", ".rooyor.", ".royyor.", "..ryyr.."],
  },
  {
    id: "tree",
    background: "#203226",
    palette: { l: "#4f8f3a", L: "#6fbf4f", t: "#7a5230" },
    rows: ["..LLLL..", ".LllLlL.", "LlLllLlL", "lLlLlllL", ".llLlll.", "...tt...", "...tt...", "..tttt.."],
  },
  {
    id: "potion",
    background: "#2c2238",
    palette: { g: "#c8d8e8", c: "#a070e0", C: "#d0a8ff", k: "#7a5230" },
    rows: ["...kk...", "...gg...", "..g..g..", ".g....g.", "gccCcccg", "gcCccccg", "gccccccg", ".gggggg."],
  },
  {
    id: "book",
    background: "#2e2420",
    palette: { b: "#8a3a2a", B: "#b05040", p: "#ece0c8", g: "#e0b040" },
    rows: [".BBBBBB.", "BbbbbbbB", "BbbggbbB", "BbbbbbbB", "BbbbbbbB", "BbbbbbbB", "Bpppppp.", ".BBBBBB."],
  },
  {
    id: "star",
    background: "#2a2a1a",
    palette: { y: "#ffd84a", Y: "#fff0a0", o: "#e0a020" },
    rows: ["...yy...", "...Yy...", "yyyYyyyy", ".yYyyyo.", "..yyyo..", ".yyooyy.", ".yo..oy.", "o......o"],
  },
  {
    id: "cube",
    background: "#1e2430",
    palette: { a: "#7aa0ff", b: "#4a70d0", c: "#2a4a9a" },
    rows: ["..aaaa..", ".aaaaaa.", "baaaaaac", "bbaaaacc", "bbbbcccc", "bbbbcccc", ".bbbccc.", "..bbcc.."],
  },
  {
    id: "moon",
    background: "#1c2236",
    palette: { y: "#f0e6a0", Y: "#d8c870" },
    rows: ["..yyy...", ".yyY....", "yyY.....", "yyY.....", "yyY.....", "yyyY....", ".yyyyY..", "..yyy..."],
  },
  {
    id: "sun",
    background: "#3a3020",
    palette: { y: "#ffd84a", o: "#f0a020" },
    rows: ["y..yy..y", ".y.yy.y.", "..yooy..", "yyooooyy", "yyooooyy", "..yooy..", ".y.yy.y.", "y..yy..y"],
  },
  {
    id: "mushroom",
    background: "#2a2a30",
    palette: { r: "#d83a3a", w: "#f4f0e8", s: "#e8dcc0", S: "#c8b898" },
    rows: ["..rrrr..", ".rwrrwr.", "rrrrrrrr", "rwrrrrwr", "..ssss..", "...sS...", "...sS...", "..sSSs.."],
  },
  {
    id: "fish",
    background: "#1c3040",
    palette: { b: "#4fa3d8", B: "#2f7fb0", e: "#101820", f: "#7cc4ec" },
    rows: ["........", "..bbbb.B", ".bebbbBB", "bbbbbbB.", "bfbbbbBB", ".bbbbb.B", "..BBB...", "........"],
  },
  {
    id: "key",
    background: "#33301c",
    palette: { g: "#e0b040", G: "#a07a20" },
    rows: ["..ggg...", ".g...g..", ".g...g..", "..gGg...", "...g....", "...gg...", "...g....", "...ggg.."],
  },
  {
    id: "shield",
    background: "#202838",
    palette: { s: "#9fb3c0", b: "#3a6fd1", B: "#2a4fa0" },
    rows: ["ssssssss", "sbbbbbbs", "sbBbbBbs", "sbbbbbbs", "sbbBBbbs", ".sbbbbs.", "..sbbs..", "...ss..."],
  },
  {
    id: "skull",
    background: "#2a2a2e",
    palette: { w: "#e8e4dc", k: "#1a1a1e", W: "#c0bcb4" },
    rows: [".wwwwww.", "wwwwwwww", "wkkwwkkw", "wkkwwkkw", "wwwkkwww", ".wwwwww.", ".wkwkwk.", "..wWwW.."],
  },
  {
    id: "cake",
    background: "#3a2a2a",
    palette: { w: "#f4f0e8", r: "#d83a3a", b: "#c08040", B: "#9a6030", c: "#f0c040" },
    rows: ["...c....", "...r....", "wwwwwwww", "wrwwrwwr", "bbbbbbbb", "BBBBBBBB", "bbbbbbbb", "BBBBBBBB"],
  },
  {
    id: "rocket",
    background: "#1c2030",
    palette: { w: "#e8ecf0", r: "#d83a3a", b: "#4fa3d8", o: "#f0a020" },
    rows: ["...ww...", "..wwww..", "..wbbw..", "..wwww..", ".rwwwwr.", "rrwwwwrr", "...oo...", "..o..o.."],
  },
  {
    id: "flower",
    background: "#22302a",
    palette: { p: "#e070b0", y: "#f0d040", g: "#4a9a3a" },
    rows: ["..p.p...", ".ppypp..", "..ppp...", "...g....", ".g.g.g..", "..ggg...", "...g....", "...g...."],
  },
  {
    id: "crown",
    background: "#302818",
    palette: { g: "#f0c040", G: "#c09020", r: "#d83a3a", b: "#4fa3d8" },
    rows: ["........", "g..g..g.", "gg.gg.gg", "gggggggg", "grggbggr", "gggggggg", "GGGGGGGG", "........"],
  },
  {
    id: "coin",
    background: "#302a18",
    palette: { y: "#f0c040", Y: "#ffe080", o: "#b08020" },
    rows: ["..yyyy..", ".yYYYYy.", "yYyyyyoy", "yYyooyoy", "yYyooyoy", "yyyyyyoy", ".yooooy.", "..yyyy.."],
  },
  {
    id: "snow",
    background: "#1c2a38",
    palette: { w: "#e8f4ff", b: "#9fd0f0" },
    rows: ["...w....", ".w.w.w..", "..www...", "wwwbwww.", "..www...", ".w.w.w..", "...w....", "........"],
  },
  {
    id: "bolt",
    background: "#2a2a1c",
    palette: { y: "#ffe040", Y: "#d0b020" },
    rows: ["....yy..", "...yy...", "..yy....", ".yyyyy..", "...yy...", "..yy....", ".yY.....", ".Y......"],
  },
  {
    id: "house",
    background: "#23302a",
    palette: { r: "#c04030", R: "#902a20", w: "#e8dcc0", d: "#7a5230", b: "#4fa3d8" },
    rows: ["...rr...", "..rrrr..", ".rrRRrr.", "rrrrrrrr", ".wwwwww.", ".wbwwdw.", ".wwwwdw.", ".wwwwdw."],
  },
  {
    id: "portal",
    background: "#1a1426",
    palette: { o: "#2a1f3a", p: "#a040e0", P: "#d080ff" },
    rows: ["oooooooo", "oppPpppo", "opPpppPo", "oppppPpo", "oPpppppo", "opppPppo", "oppPpppo", "oooooooo"],
  },
  {
    id: "bow",
    background: "#2a2620",
    palette: { w: "#9a6a3a", s: "#e8e4dc", a: "#9fb3c0" },
    rows: ["..ww....", ".w..s...", "w...s...", "w...saaa", "w...s...", ".w..s...", "..ww....", "........"],
  },
  {
    id: "pumpkin",
    background: "#2e2418",
    palette: { o: "#e08020", O: "#b05a10", k: "#2a1a08", g: "#4a9a3a" },
    rows: ["...g....", ".oOoOoo.", "oOoOoOoo", "okkOokko", "oOoOoOoo", "okOkkOko", ".oOoOoo.", "........"],
  },
];

export const presetById = (id: string) => iconPresets.find((p) => p.id === id);
