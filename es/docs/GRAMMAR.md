# Schema grammar (draft v1)

Формат описания схемы YAML-документа для редактирования в TUI.

Схема повторяет дерево документа: листья задают, как редактировать значение;
вложенные мапы — группы, которые можно раскрыть.

Схема и данные — разные файлы. Схема не является валидным документом данных.

---

## Две оси

| Ось | Смысл | Компактная форма | Полная форма |
|-----|--------|------------------|--------------|
| **тип значения** | что пишется в выходной YAML | префикс до `:` | `$type` |
| **виджет** | как поле выглядит в TUI | `widget=` | `$widget` |

Правила по умолчанию:

- если тип значения не указан → **`string`**;
- если `$default` / `default=` нет и ключ в данных не задан → значение **undefined**: ключ **не пишется** в выходной YAML (в том числе для `boolean`).

---

## Дерево схемы

- **Скаляр-строка** → описание листа (компактный DSL).
- **Мапа без `$widget`** → вложенный объект (группа). В TUI раскрывается.
- **Мапа с `$widget`** → лист в полной форме.
- Ключи, начинающиеся с `$`, — метаданные схемы, в документ данных не попадают.

Порядок полей в TUI = порядок ключей в схеме (YAML его сохраняет).

Ограничение v1: в редактируемых данных не ожидаются ключи вида `$…`.

---

## Компактный DSL (лист)

```
leaf := value_type ":" params
      | "boolean"
```

`params` — список `key=value` через запятую. Обязательный ключ — `widget`.

```
params := param ("," param)*
param  := key "=" value
```

В компактной форме значения параметров **не содержат запятых**.

### Примеры

```yaml
name: string:widget=text,default=John,label=Имя
password: string:widget=password
description: string:widget=textarea,height=4
color: string:widget=select,options=Red(1)|Green(2)|Blue(3)
present: boolean
```

Сокращение: `boolean` ≡ `boolean:widget=checkbox` (тип значения `boolean`, виджет `checkbox`).

### Параметры компактной формы

| Ключ | Смысл | Где применимо |
|------|--------|----------------|
| `widget` | виджет | все листья |
| `default` | значение по умолчанию | любой лист |
| `label` | подпись в UI | любой лист |
| `height` | высота в строках | `textarea` |
| `options` | варианты выбора | `select` |
| `on` | подпись для `true` (по умолчанию `enabled`) | `enable` |
| `off` | подпись для `false` (по умолчанию `disabled`) | `enable` |
| `empty` | текст свёрнутого вида без пароля (по умолчанию `empty`) | `password` |
| `filled` | текст свёрнутого вида при заданном пароле (по умолчанию `provided`) | `password` |

Формат `options`:

```
options := option ("|" option)*
option  := label "(" value ")"
         | label
```

Если `(value)` нет — value = label. Тип записанного value берётся из `$type` / префикса поля (`string` → `"1"`, `number` → `1`).

---

## Полная форма (лист или группа с метаданными)

Ключи метаданных:

| Ключ | Смысл |
|------|--------|
| `$type` | тип значения: `string`, `boolean`, `number`, … |
| `$widget` | виджет TUI |
| `$default` | значение по умолчанию (настоящий YAML-скаляр/структура) |
| `$label` | подпись |
| `$help` | подсказка (опционально) |
| `$options` | список вариантов для `select` |
| `$height` | высота `textarea` |
| `$on` / `$off` | подписи для виджета `enable` |
| `$empty` / `$filled` | подписи свёрнутого вида для `password` |

Эквивалентность компактной и полной формы:

```yaml
# компактно
name: string:widget=text,default=John,label=Имя

# полностью
name:
  $type: string
  $widget: text
  $default: John
  $label: Имя
```

Полная форма нужна, когда компактной мешают запятые, сложные default или селект:

```yaml
name:
  $type: string
  $widget: text
  $default: "John, Jr."
  $label: Имя

color:
  $type: string
  $widget: select
  $options:
    - label: Red
      value: "1"
    - label: Green
      value: "2"
    - label: Blue
      value: "3"
```

Группа с подписью (без `$widget` — это объект):

```yaml
sides:
  $label: Стороны
  left: boolean
  right:
    $type: boolean
    $widget: checkbox
    $default: true
    $label: Справа
```

---

## Виджеты v1

| `$widget` / `widget=` | `$type` по смыслу | TUI |
|-----------------------|-------------------|-----|
| `text` | `string` (обычно) | одна строка |
| `password` | `string` | маскированный ввод; свёрнуто — `empty`/`filled` (по умолчанию `empty`/`provided`); в файле — открытый текст |
| `textarea` | `string` | много строк; `height` / `$height` — только UI |
| `select` | `string` или `number` | выбор из списка; в файл пишется value |
| `checkbox` | `boolean` | флажок, без раскрытия |
| `enable` | `boolean` | тот же bool, что `checkbox`, но UI — подписи `enabled`/`disabled` (или `on`/`off`); раскладка (одна активная подпись vs обе рядом) — решение UI |

Раскрываются только композиты (вложенные мапы / в будущем списки), не скаляры.

Пример `enable`:

```yaml
feature: boolean:widget=enable
feature_ru: boolean:widget=enable,on=вкл,off=выкл
```

Пример `password`:

```yaml
password: string:widget=password
secret: string:widget=password,empty=не задан,filled=задан
```

---

## Слияние схемы и данных

1. Ключ есть в данных → берём значение из данных.
2. Ключа нет, есть `$default` / `default=` → показываем default в UI; при сохранении **пишем** ключ.
3. Ключа нет и default нет → **undefined**: в UI «не задано»; при сохранении ключ **пропускаем**.

То же для `boolean`: нет default и ключ не задан → omit, не `false`.

### Пустые значения (открытый вопрос v1)

- **Нетронутое** поле без default → omit.
- **Явно очищенная** строка — кандидат на `""` vs omit; поведение зафиксировать при реализации.
- Аналогично для будущего пустого списка: `[]` vs omit.

---

## Пример схемы (как в `../f.yaml`)

```yaml
name: string:widget=text,default=John,label=Имя
password: string:widget=password
description: string:widget=textarea,height=4
color: string:widget=select,options=Red(1)|Green(2)|Blue(3)
present: boolean
sides:
  left: boolean
  right: boolean
```

Возможный документ после редактирования (если трогали только часть полей):

```yaml
name: John
color: "1"
sides:
  left: true
```

`password`, `description`, `present`, `sides.right` отсутствуют — undefined.

---

## Списки (набросок, не обязательно v1)

Список — композит: раскрывается, элементы однотипны.

```yaml
tags:
  $type: list
  $item: string:widget=text

servers:
  $type: list
  $item:
    host: string:widget=text
    port: number:widget=text,default=22
  $default:
    - host: localhost
      port: 22
```

В TUI: свёрнуто — `tags [3]` / пусто; раскрыто — элементы, add/remove; элемент-объект раскрывается как обычная группа.

Заметки:

- `$item` один (heterogeneous list — вне scope).
- Компактный DSL для list почти бесполезен при объектном `$item` → list в основном полной формой.
- Нет ключа и нет `$default` → undefined (omit); явный `[]` — уже заданное значение.
- Свободная мапа с произвольными ключами — не list; при необходимости отдельный `$type: map` + схема значения.

---

## Вне scope v1

- JSON Schema целиком.
- Валидация (`required`, regex, min/max) — отдельный слой поверх виджетов.
- Числа как отдельный виджет UI (можно хранить через `$type: number` + `text`/`select`).
- Списки и динамические мапы — после стабилизации листьев и групп.

---

## Краткая сводка

1. Префикс до `:` / `$type` — тип значения в YAML; по умолчанию `string`.
2. `widget=` / `$widget` — виджет TUI.
3. Нет default и ключ не задан → undefined → ключ omit (включая boolean).
4. Вложенная мапа без `$widget` → раскрываемая группа.
5. Компактный DSL оставляем; полная форма — для сложных значений параметров.
6. Списки — `$type: list` + `$item`, продумать заранее, внедрять позже.
