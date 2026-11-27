<template>
  <div class="field">
    <label>{{ label }}</label>
    <el-input-number v-if="type === 'number'" :model-value="modelValue" :min="0" style="width:100%"
      @update:model-value="$emit('update:modelValue', $event)" />
    <el-input v-else :model-value="modelValue" :type="type === 'password' ? 'password' : 'text'"
      :placeholder="placeholder || ''" style="width:100%"
      @update:model-value="$emit('update:modelValue', $event)" />
  </div>
</template>

<script setup>
defineProps({
  label: String,
  modelValue: { type: [String, Number], default: '' },
  type: String,
  placeholder: String,
});
defineEmits(['update:modelValue']);
</script>

<style scoped>
.field { display: flex; flex-direction: column; gap: 7px; min-width: 0; }
.field label { font-size: 13px; font-weight: 600; color: var(--cm-text-2); }
</style>
