FROM node:22-slim AS build
WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci || npm install
COPY . .
RUN VITE_VESTRACE_WORKSPACE_ID=10000000-0000-0000-0000-000000000001 \
    VITE_VESTRACE_PRINCIPAL_ID=10000000-0000-0000-0000-000000000002 \
    npm run build

FROM nginx:alpine
COPY --from=build /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 3000
