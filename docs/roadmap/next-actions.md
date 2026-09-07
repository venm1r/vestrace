# Ближайшие задания и порядок принятия

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предложенная последовательность; чекбоксы намеренно не отмечены.

## Первое окно работы

Не начинать все группы сразу. Рекомендуется один implementation package и одна независимая documentation/measurement задача.

### A. Зафиксировать текущий срез

- [ ] Сверить локальный HEAD с 07e2977 или записать более новый осознанный baseline.
- [ ] Перечитать P04 evidence после 14D и проверить, не реализована ли уже 14E на другой ветке.
- [ ] Снять dirty/untracked snapshot и exact writable/protected paths.
- [ ] Принять, что этот roadmap не меняет frozen scope; определить отдельное решение о MW milestone.

Проверка read-only в реальном checkout:

```bash
git rev-parse HEAD
git status --porcelain=v1 -z
git diff --name-status 6f6102536e9a535b7086db14573bf45fe750ad71 07e2977a20b05c5b16953a206a6d68bdbff3a052
```

Не публиковать raw dirty file contents, если там есть credentials или пользовательские данные.

### B. Закончить текущую принятую фундаментальную задачу

- [ ] Использовать существующий detailed P04 continuation, не черновую перепись этого roadmap.
- [ ] Проверить роли, outcome identity, current guards и crash-recovery без второго dispatch.
- [ ] Не смешивать новый MW migration с незавершённым 14E без принятого shared scope.

### C. Подготовить первый MW вертикальный срез

- [ ] Выполнить [MW-00](../implementation/memory-workspace/plans/00-preflight.md).
- [ ] Начать read/detail/history из MW-01, подключив настоящий query port.
- [ ] До editor acceptance довести MW-02 atomic writer; не исправлять отсутствие API доступом Console к БД.
- [ ] Выбрать синтетический пример для read→edit→reload, а не сразу importer всех форматов.

### D. Измерение без влияния на кодовый scope

- [ ] Записать cold build, локальный test cycle, RAM/disk и blockers среды.
- [ ] Разметить исходный corpus current/old/corrected/no-evidence; все product results оставить NOT_RUN.
- [ ] Подготовить инструкции токена/Console без передачи credential в browser bundle.

## Что потребуется от исполнителя на выходе

Exact changed files, требование и failing/passing observation, итоговый SHA, runtime environment, checks not run, reviewer verdict и продолжение очереди. Простая отметка «сделано» не закрывает ни milestone, ни spec.

Это план подготовки и handoff. Детальные RED→GREEN steps с конкретными интерфейсами для P1 уже находятся в восьми MW-планах; здесь они намеренно не дублируются.

---
**Основание:** [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R18: docs/implementation/memory-workspace/README.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/implementation/memory-workspace/README.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
