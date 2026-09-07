# Alpha, full v1.0 и правила выпуска

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Три независимых решения

Номер версии, список доступных функций и qualification verdict должны согласовываться, но не заменяют друг друга. Большое число тестов не означает API stability. Модель `0.1.0-alpha.1` — предлагаемое имя раннего публичного испытания; оно не назначается этим документом и не устанавливает дату.

## Ограниченная alpha

Может включать узкий memory-first сценарий, если явно утверждён shipping manifest, сохранены общие safety gates, описаны supported environment/данные и ограничения. Она не называется полным v1.0 и не обещает недоведённые AG-UI/A2A или unrestricted production usage.

Для alpha нужны реальная установка, чтение/изменение/история, объявленный импортный сценарий, сохранность при выбранных сбоях и отрицательные проверки доступа. Любая включённая функция должна иметь свою acceptance; отсутствие меню не освобождает общие используемые authorities от проверки.

## Полный действующий v1.0

Frozen P01–P12 остаётся обязательным планом до отдельного amendment. P06–P11 нельзя выкинуть из v1.0 только потому, что в продуктовой очереди они помечены P3. P12 требует fresh exact-environment evidence. MW не становится частью v1.0 автоматически от наличия этой дорожной карты.

## Release checklist

Нужны exact source/build/images/config/model/protocol identities; выполненные tests; limitations; миграция с существующей версии; проверка restore; правила compatibility; доступный канал report. Пропущенная, blocked или неисполненная проверка не считается PASS.

Release notes перечисляют, что пользователь теперь может сделать, какую подготовку нужно выполнить и какой scope не поддерживается. Не помещать credentials, fixture административные обходы и маркетинговую оценку архитектуры как proof.

## Контракт до 1.0

Перед изменением API/data/schema описать breaking surface и transition policy. Pre-1.0 не является разрешением незаметно портить данные. История ревизий и экспорт/restore остаются важными даже при изменяемом внешнем API.

---
**Основание:** [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
