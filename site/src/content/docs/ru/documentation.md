---
title: Документация
template: splash
---

Запустите рабочий пример из корня репозитория:

```sh
cargo run -p hello-world --locked
```

Результат вычисляется после четырёх шагов симуляции и запроса к полю со знаком. Начните с [контрактов полей](../reference/specifications/mathematical-fields/), [оценок расстояния](../reference/specifications/distance-bounds/), [пространственных запросов](../reference/specifications/spatial-queries/) и [состояния мира](../reference/specifications/world-state/). API-документация создаётся из исходного кода командой `cargo doc --workspace --no-deps --locked`.

GPU-окно запускается через `cargo run -p first-light --locked`; подробности — в [руководстве First Light](../reference/architecture/first-light/) и [контракте CPU/GPU](../reference/specifications/cpu-gpu-contract/). GPU-тесты запускаются отдельно и требуют совместимого адаптера.

Для прототипа роста используйте `cargo run -p first-life --locked -- baseline`, `pruning` или `pruning-replay`. Каждая команда выводит машиночитаемое состояние. Команда `cargo run -p first-light --locked -- --life` открывает окно: щёлкните по ветви, нажмите **P** для обрезки на следующем тике, **Space** для паузы или продолжения. См. [контракт взаимодействия](../reference/specifications/first-interaction/), [модель роста](../reference/specifications/growth-model/) и [спецификацию сохранения](../reference/specifications/world-persistence/).

Foundation 0.5 добавляет ограниченное ветвление и аналитические соединения-капсулы. Команда `cargo run -p first-life --locked -- scale --measure` измеряет локальное масштабирование CPU. Ограничения и результаты приведены в [контракте капсулы](../reference/specifications/capsule/) и [записи измерений First Structure](../reference/research/first-structure-measurements/). Если мир превышает бюджет GPU-снимка, программа сообщает об этом явно.
