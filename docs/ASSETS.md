# ORC-WAR 애셋 명세 (이미지 생성 프롬프트)

> v0.1 · 2026-08-27 · 이 문서의 프롬프트를 이미지 생성기에 넣어 애셋을 조달한다.

## 0. 공통 규격 (중요)

- **개발은 애셋 없이 진행 가능** — M1~M4는 절차 생성 실루엣으로 돌아가고, 이 애셋들은 M5(폴리시)에서 교체 투입된다. 여유 있게 준비하면 된다.
- **납품 형식**: PNG, 투명 배경(불가하면 단색 마젠타 `#FF00FF` 배경 → 내가 키잉 처리).
- **시점**: 완전 수직 탑뷰가 아니라 **약간 기울어진 오버헤드(75~80°)** — 『대장님, 오크가 몰려옵니다』처럼 머리·어깨 위주로 보이되 무기 실루엣이 식별되는 각도.
- **유닛 방향**: 전부 **위(북쪽)를 바라보는 1방향**만. 엔진에서 회전·플립 처리한다.
- **팀 컬러**: 옷·방패 등 천 부분은 **순수한 밝은 빨강(#FF0000)** 으로 통일 → 엔진에서 색상 치환(hue swap)으로 적/청 팀을 만든다.
- **해상도**: 유닛은 생성 후 인게임 8~16px로 다운스케일된다. **실루엣이 뭉개지지 않는 단순한 형태**가 최우선. 디테일은 사치다.
- **프레임**: 정지 1프레임이면 충분. (걷기 까딱임은 엔진에서 스프라이트를 흔들어 처리)

### 전 프롬프트 공통 접두어 (모든 프롬프트 앞에 붙일 것)

```
16-bit pixel art game sprite, top-down overhead view at a slight angle,
single sprite centered, transparent background, crisp clean silhouette,
limited color palette, flat lighting, no text, no watermark, no shadows on ground
```

---

## 1. 유닛 스프라이트 — 8종 (우선순위: 필수)

생성 크기 64×64. 파일명 규칙: `units/<이름>.png`

| 파일명 | 프롬프트 (공통 접두어 뒤에) |
|---|---|
| `inf_sword` | a medieval swordsman seen from above, facing up, round iron helmet, pure bright red (#FF0000) tunic, large round shield held on the left side, short sword in right hand, 64x64 |
| `inf_spear` | a medieval pikeman seen from above, facing up, simple conical helmet, pure bright red (#FF0000) tunic, holding a very long spear pointing straight up so the long shaft reads clearly in silhouette, no shield, 64x64 |
| `inf_axe` | a heavily armored medieval soldier seen from above, facing up, full steel plate armor and great helm, pure bright red (#FF0000) cape detail, holding a large two-handed battle axe across the body, bulky silhouette, 64x64 |
| `archer` | a medieval archer seen from above, facing up, leather cap, pure bright red (#FF0000) tunic, holding a curved longbow in the left hand and drawing an arrow, quiver on the back visible, slim silhouette, 64x64 |
| `crossbow` | a medieval crossbowman seen from above, facing up, kettle helmet, pure bright red (#FF0000) tunic, holding a heavy crossbow pointed up, wide T-shaped weapon silhouette, 64x64 |
| `cav_light` | a light cavalry rider on a horse seen from above, horse facing up, unarmored brown horse, rider with pure bright red (#FF0000) tunic holding a curved saber, slender fast-looking silhouette, horse body clearly longer than a man, 64x64 |
| `cav_heavy` | a heavy knight on an armored warhorse seen from above, horse facing up, horse covered in steel barding with pure bright red (#FF0000) caparison, rider in full plate holding a very long lance pointing up past the horse's head, massive bulky silhouette, 64x64 |
| `cav_horse_archer` | a horse archer on a light steppe pony seen from above, horse facing up, rider with pure bright red (#FF0000) coat turning slightly to shoot a short recurve bow, quiver on the horse's flank, 64x64 |

## 2. 공성 병기 — 5종 (필수)

생성 크기 128×128 (사다리만 128×32). 파일명 `siege/<이름>.png`

| 파일명 | 프롬프트 |
|---|---|
| `ladder` | a long wooden siege ladder lying flat seen directly from above, two rails and many rungs, weathered wood, simple, 128x32 |
| `siege_tower` | a tall wooden siege tower seen from above at a slight angle, square wooden frame with a flat top platform, hide-covered sides, four wheels visible at the corners, facing up, 128x128 |
| `ram` | a covered battering ram seen from above, long wooden gable roof shed on wheels covering the ram, rough timber and hide texture, facing up so the long axis is vertical, 128x128 |
| `catapult` | a medieval catapult (mangonel) seen from above at a slight angle, wooden frame with a throwing arm and sling laid toward the bottom, counterweight, four wheels, facing up, 128x128 |
| `ballista` | a giant crossbow ballista on a wooden mount seen from above, wide bow arms spanning left to right, a heavy bolt loaded pointing up, two wheels, 128x128 |

## 3. 지형 — 타일 텍스처 (필수)

시임리스 타일 256×256. 파일명 `terrain/<이름>.png`
프롬프트 접두어 교체: `16-bit pixel art seamless tileable ground texture, top-down view, muted natural colors, no text, no watermark`

| 파일명 | 프롬프트 |
|---|---|
| `grass` | short dry grassland seen from directly above, subtle tufts and dirt patches, olive green, seamless tile, 256x256 |
| `dirt` | packed dry dirt and sparse gravel seen from directly above, dusty brown, seamless tile, 256x256 |
| `forest_floor` | dark forest ground with roots and fallen leaves seen from directly above, deep green-brown, seamless tile, 256x256 |
| `rock` | grey rocky mountain surface with cracks seen from directly above, seamless tile, 256x256 |
| `water` | river water seen from directly above, dark blue-green with subtle ripple highlights, seamless tile, 256x256 |
| `ford` | shallow river ford seen from directly above, water over visible pebbles and sand, lighter than deep water, seamless tile, 256x256 |
| `sand_moat` | dry moat bottom of dirt and sharpened wooden stakes seen from directly above, seamless tile, 256x256 |

개별 오브젝트 (투명 배경, 유닛 접두어 사용):

| 파일명 | 프롬프트 |
|---|---|
| `tree` | a single round tree canopy seen from directly above, dense dark green foliage with slight highlight on one side, roughly circular, 96x96 |
| `boulder` | a single large grey boulder seen from above at a slight angle, cracked granite, irregular round shape, 96x96 |

## 4. 성 구조물 (필수)

파일명 `castle/<이름>.png`. 성벽은 가로로 이어붙일 수 있어야 한다(좌우 시임리스).

| 파일명 | 프롬프트 |
|---|---|
| `wall_straight` | a stone castle wall segment seen from above at a slight angle, walkway running horizontally with crenellated battlements on the top and bottom edges, grey stone blocks, horizontally tileable, 128x96 |
| `wall_corner` | a square stone castle wall corner tower seen from above at a slight angle, crenellated round top platform, grey stone, 128x128 |
| `gatehouse` | a castle gatehouse seen from above at a slight angle, two square towers flanking a closed wooden double gate with iron bands, grey stone, gate facing down, 192x128 |
| `gate_broken` | the same wooden castle double gate but smashed open, splintered planks and debris, seen from above, 128x96 |
| `wall_rubble` | a collapsed stone wall breach seen from above, pile of grey rubble forming a rough ramp through the gap, dust, horizontally tileable with wall segments, 128x96 |
| `bridge` | a wooden plank bridge over a moat seen from directly above, heavy timber planks running horizontally, vertical orientation, 96x128 |
| `keep` | a square stone castle keep seen from above at a slight angle, slate roof with pure bright red (#FF0000) banner on top, 192x192 |

## 5. 시체·데칼 (권장)

투명 배경. 파일명 `decals/<이름>.png`. 각각 변형 2~3개면 좋다.

| 파일명 | 프롬프트 |
|---|---|
| `corpse_inf` | a fallen medieval soldier lying on the ground seen from directly above, sprawled pose, dropped sword beside the body, muted colors, 64x64 |
| `corpse_archer` | a fallen archer lying on the ground seen from directly above, dropped bow and scattered arrows, 64x64 |
| `corpse_horse` | a fallen warhorse lying on its side seen from directly above, rider sprawled next to it, 96x96 |
| `blood_1..3` | a dark red blood splatter stain seen from directly above, irregular splash shape, semi-transparent edges, 48x48 (3 variants, different shapes) |
| `crater` | a small impact crater with scattered stone debris seen from directly above, for catapult hits, 64x64 |

## 6. UI / 기타 (선택)

| 파일명 | 프롬프트 |
|---|---|
| `ui/icon_<병종>` | (병종별 8개) a square game UI icon portrait of a medieval <swordsman/pikeman/...>, bust view from the front, pixel art, dark background, thin gold border, 64x64 |
| `ui/title` | pixel art game title illustration, a massive medieval battlefield seen from above with two huge armies clashing and a burning castle, dramatic, wide 16:9, no text |

- 발사체(화살·투석 돌·볼트), 먼지·불 이펙트는 **코드로 그리는 게 더 깔끔해서 필요 없음**.
- 폰트는 오픈소스(Galmuri 등) 사용 예정, 조달 불필요.
- 오디오는 별도 논의(이 문서는 이미지만).

## 7. 체크리스트 요약

| 분류 | 수량 | 우선순위 |
|---|---|---|
| 유닛 8종 | 8 | 필수 |
| 공성병기 | 5 | 필수 |
| 지형 타일 7 + 오브젝트 2 | 9 | 필수 |
| 성 구조물 | 7 | 필수 |
| 시체·데칼 | 7~9 | 권장 |
| UI | 9 | 선택 |
