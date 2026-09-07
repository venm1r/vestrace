# Развёртывание: границы и готовность компонентов

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Поддерживаемый первый путь

Рекомендуемая первая цель дорожной карты — одна повторяемая локальная self-hosted среда. Это проект критерия, не новый сертификат поддерживаемой платформы. CI image tags и schema compatibility сами по себе не квалифицируют filesystem, vault, network policy и модель.

Состав: PostgreSQL и provisioning/migrate, server, worker, Console proxy, постоянные vault roots и external read-only bootstrap secret store. Должно быть ясно, кто создаёт identities/grants, кто имеет право мигрировать и кто запускает runtime.

## Таблица readiness

| Уровень | Что проверять | Чего он не доказывает |
| --- | --- | --- |
| Процесс | Liveness и возможность отвечать | Доступность БД и полезной задачи |
| Schema/storage | Reachability и миграционная совместимость | Наличие модели, прав и готовой generation |
| Authority | Token, grants, disclosure settings, vault roots | Успех конкретного внешнего вызова |
| Функция | Полный memory/retrieval/provider сценарий | Квалификацию остальных функций |
| Релиз | Все required evidence на точном target | Поддержку другой непроверенной среды |

## Перед работой с ценными данными

Записать target identities, описать backup/restore, проверить key custody и сетевые границы. Отдельно установить retention/logging policy. Нельзя считать isolated developer Compose готовой корпоративной topology.

Каждый процесс использует тот же принятый source/image и совместимый storage contract. Не развёртывать отдельно более новый worker с непроверенным старым server только потому, что оба стартуют.

## Deployment changes

Новые frontend routes не расширяют допуск по умолчанию. Admission destination/type model задаётся явной Connection revision. Документация не должна превращать произвольный URL из импортированного файла в server-side fetch.

Для добавления hosted deployment см. [P4](../roadmap/p4-expansion.md); это отдельная программа после валидации основных сценариев.

---
**Основание:** [R04: docker-compose.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml), [R05: docs/getting-started.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/getting-started.md), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R16: apps/console/nginx.conf.template](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/nginx.conf.template).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
