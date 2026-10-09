/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        synapse: {
          reference: '#3B82F6',
          derived: '#10B981',
          contradicts: '#EF4444',
          supersedes: '#F97316',
          extends: '#A855F7',
          custom: '#6B7280',
        },
      },
      fontFamily: {
        system: [
          '-apple-system',
          'BlinkMacSystemFont',
          'SF Pro Display',
          'Segoe UI',
          'sans-serif',
        ],
      },
    },
  },
  plugins: [],
};
