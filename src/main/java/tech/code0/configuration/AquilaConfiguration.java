package tech.code0.configuration;

import lombok.Getter;

import static tech.code0.util.AquilaLogger.LOGGER;

@Getter
public class AquilaConfiguration {

    private final String sessionToken;
    private final String backendHost;
    private final String rabbitMQHost;
    private final String redisHost;

    private final int backendPort;
    private final int rabbitMQPort;
    private final int redisPort;

    public AquilaConfiguration() {
        LOGGER.info("Initializing environment variables");
        this.sessionToken = getEnvVar("SESSIONTOKEN");
        this.backendHost = getEnvVar("BACKENDHOST");
        this.rabbitMQHost = getEnvVar("RABBITMQHOST");
        this.redisHost = getEnvVar("RABBITMQHOST");

        this.rabbitMQPort = Integer.parseInt(getEnvVar("RABBITMQPORT"));
        this.redisPort = Integer.parseInt(getEnvVar("RABBITMQPORT"));

        final var port = System.getenv("BACKENDPORT");
        this.backendPort = (port != null) ? Integer.parseInt(port) : 0;
    }

    private String getEnvVar(String varName) {
        String value = System.getenv(varName);

        if (value == null) {
            LOGGER.warning("Environment variable " + varName + " is undefined.");
            throw new IllegalArgumentException("Environment variable '" + varName + "' not found.");
        }

        return value;
    }

}