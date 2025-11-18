<script setup lang="ts">
import { ref } from 'vue';
import Login from './components/Login.vue';
import MainView from './components/MainView.vue';

const isConnected = ref(false);

const handleConnected = () => {
  isConnected.value = true;
};

const handleDisconnected = () => {
  isConnected.value = false;
};
</script>

<template>
  <div id="app">
    <Login v-if="!isConnected" @connected="handleConnected" />
    <MainView v-else @disconnect="handleDisconnected" />
    </div>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  margin: 0;
  padding: 0;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

#app {
  width: 100vw;
  height: 100vh;
  overflow: hidden;
}
</style>
