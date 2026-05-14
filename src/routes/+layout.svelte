<script lang="ts">
  import "../app.css";

  let { children } = $props();
</script>

<div class="app">
  {@render children()}
</div>

<style>
  .app {
    width: 100vw;
    height: 100vh;
    display: flex;
    overflow: hidden;
  }
</style>
