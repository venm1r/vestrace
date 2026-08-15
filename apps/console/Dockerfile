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
# The nginx entrypoint expands templates into /etc/nginx/conf.d at start-up, so
# the token is supplied at run time rather than baked into the image layers.
# The filter keeps envsubst away from nginx's own `$host`, `$remote_addr` and
# friends, which are not environment variables.
COPY nginx.conf.template /etc/nginx/templates/default.conf.template
ENV NGINX_ENVSUBST_FILTER="VESTRACE_"
EXPOSE 3000
