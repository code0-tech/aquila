package tech.code0.configuration;

import lombok.Getter;

import java.util.logging.Logger;

public class AquilaConfiguration {

    @Getter private final String sessionToken;
    @Getter private final String backendHost;
    @Getter private final String rabbitMQHost;
    @Getter private final String redisHost;

    @Getter private final int backendPort;
    @Getter private final int rabbitMQPort;
    @Getter private final int redisPort;

    public final Logger logger;

    public AquilaConfiguration() {
        this.logger = Logger.getLogger(AquilaConfiguration.class.getName());

        this.logger.info("Initializing environment variables");
        this.sessionToken = getEnvVar("SESSION_TOKEN");
        this.backendHost = getEnvVar("BACKEND_HOST");
        this.rabbitMQHost = getEnvVar("RABBITMQ_HOST");
        this.redisHost = getEnvVar("RABBITMQ_HOST");

        this.rabbitMQPort = Integer.parseInt(getEnvVar("RABBITMQ_PORT"));
        this.redisPort = Integer.parseInt(getEnvVar("RABBITMQ_PORT"));

        final var port = System.getenv("BACKEND_PORT");
        this.backendPort = (port != null) ? Integer.parseInt(port) : 0;
    }

    private String getEnvVar(String varName) {
        String value = System.getenv(varName);

        if (value == null) {
            this.logger.warning("Environment variable " + varName + " is undefined.");
            throw new IllegalArgumentException("Environment variable '" + varName + "' not found.");
        }

        return value;
    }

}