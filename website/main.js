// Videos play only while visible; with reduced motion they stay paused with controls.
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
const videos = [...document.querySelectorAll('video')];

if (!reduced && 'IntersectionObserver' in window) {
  document.documentElement.classList.add('js');
  const reveal = new IntersectionObserver(entries => {
    for (const e of entries) if (e.isIntersecting) { e.target.classList.add('in'); reveal.unobserve(e.target); }
  }, { rootMargin: '0px 0px -8% 0px' });
  document.querySelectorAll('.reveal').forEach(el => reveal.observe(el));

  const play = new IntersectionObserver(entries => {
    for (const e of entries) {
      if (e.isIntersecting) e.target.play().catch(() => {});
      else e.target.pause();
    }
  }, { threshold: 0.35 });
  videos.forEach(v => play.observe(v));
} else if (reduced) {
  videos.forEach(v => { v.removeAttribute('autoplay'); v.pause(); v.controls = true; });
}

const trailer = document.getElementById('trailer');
const sound = document.getElementById('sound');
if (sound) sound.addEventListener('click', () => {
  trailer.muted = !trailer.muted;
  if (!trailer.muted) trailer.play().catch(() => {});
  sound.setAttribute('aria-pressed', String(!trailer.muted));
  sound.querySelector('span').textContent = trailer.muted ? 'Sound off' : 'Sound on';
  sound.querySelector('.on').hidden = trailer.muted;
  sound.querySelector('.off').hidden = !trailer.muted;
});
