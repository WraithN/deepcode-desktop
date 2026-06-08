#!/bin/bash

npx ast-grep scan -r .rules/SelectItem.yml

npx ast-grep scan -r .rules/contrast.yml

npx ast-grep scan -r .rules/toast-hook.yml

npx ast-grep scan -r .rules/slot-nesting.yml

npx ast-grep scan -r .rules/require-button-interaction.yml
