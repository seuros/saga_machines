# frozen_string_literal: true

require_relative 'saga_machines/version'
# Native is required: no algorithm fallback.
require 'saga_machines_native/saga_machines_native'

# Every value mirrors the core type of the same name, field for field.
module SagaMachines
  # The core's `SagaId`: the workflow's identity in the dedup namespace, an
  # Integer in 0..2**64-1 the caller owns. `SagaId.new(raw)` is
  # `SagaId::from_raw`.
  SagaId = Data.define(:raw) do
    # `SagaId::from_bytes`: a stable identity from an external name (FNV-1a
    # over the String's bytes, the same in every process).
    def self.from_bytes(name) = new(Saga.__send__(:native_from_bytes, name))
  end

  # The core's `AttemptId`: the handle of one try, issued once per saga and
  # never again. `raw` is its Integer form, the saga's scope in the top 16
  # bits above a sequence starting at 1. `AttemptId.new(raw)` is
  # `AttemptId::from_raw`: any Integer builds one, and the saga answers
  # :stale to one it never issued.
  AttemptId = Data.define(:raw) do
    # The scope of the saga that issued this attempt.
    def scope = Saga.__send__(:native_scope_of, raw)
  end

  # The core's `ActionKey`: which saga (a SagaId), which step, which kind
  # (:forward or :compensate). The same for every attempt at that work, before
  # and after a restart: the identity to deduplicate the effect by.
  ActionKey = Data.define(:saga, :step, :kind)

  # The core's `Action`: one attempt at one piece of work, the stable
  # ActionKey plus the AttemptId issued for this try. Plain data: store it
  # before running the effect, rebuild it when the reply arrives.
  Action = Data.define(:key, :attempt) do
    def step = key.step
    def kind = key.kind
  end

  # The core's `Progress::Forward(step)` and `Progress::Compensating(step)`.
  # The other two variants are the symbols :complete and :aborted.
  Forward = Data.define(:step)
  Compensating = Data.define(:step)

  # The core's `Settlement`: the last action resolved and the outcome
  # (:success or :failure) it was resolved with.
  Settlement = Data.define(:action, :outcome)

  # The core's `Snapshot`: everything a saga saves, in the core's field
  # order. `progress` is a Forward, a Compensating, :complete or :aborted;
  # `settled` a Settlement or nil.
  Snapshot = Data.define(:id, :scope, :progress, :issued, :settled)

  # Data values to and from the flat rows the native methods speak.
  module Wire
    module_function

    def expect(value, type)
      return value if value.is_a?(type)

      raise TypeError, "expected a #{type.name}"
    end

    def action_row(action)
      key = expect(expect(action, Action).key, ActionKey)
      [expect(key.saga, SagaId).raw, key.step, key.kind, expect(action.attempt, AttemptId).raw]
    end

    def action(row)
      saga, step, kind, attempt = row
      Action.new(ActionKey.new(SagaId.new(saga), step, kind), AttemptId.new(attempt))
    end

    def progress_row(progress)
      case progress
      when Forward then [:forward, progress.step]
      when Compensating then [:compensating, progress.step]
      when :complete, :aborted then [progress, nil]
      when Symbol then raise ArgumentError, 'progress must be a Forward, a Compensating, :complete or :aborted'
      else raise TypeError, 'expected a Forward, a Compensating, :complete or :aborted'
      end
    end

    def progress(name, step)
      case name
      when :forward then Forward.new(step)
      when :compensating then Compensating.new(step)
      else name
      end
    end

    def settled_row(settled)
      return if settled.nil?

      [*action_row(expect(settled, Settlement).action), settled.outcome]
    end

    def settled(row)
      row && Settlement.new(action(row.first(4)), row.last)
    end

    def snapshot_row(snapshot)
      expect(snapshot, Snapshot)
      [expect(snapshot.id, SagaId).raw, snapshot.scope, progress_row(snapshot.progress),
       snapshot.issued, settled_row(snapshot.settled)]
    end

    def snapshot(row)
      id, scope, progress, issued, settled = row
      Snapshot.new(SagaId.new(id), scope, progress(*progress), issued, settled(settled))
    end
  end
  private_constant :Wire

  class Saga
    class << self
      # The core's `Saga::<STEPS>::new(id)`: a fresh workflow in scope zero.
      def new(steps, id) = with_scope(steps, id, 0)

      # The core's `Saga::<STEPS>::with_scope(id, scope)`: a fresh workflow
      # whose attempts carry `scope`, an Integer in 0..65_535.
      def with_scope(steps, id, scope)
        native_with_scope(steps, Wire.expect(id, SagaId).raw, scope)
      end

      # The core's `Saga::<STEPS>::restored(snapshot, scope)`. RangeError
      # where the core answers None: a snapshot of another scope, or one no
      # saga of these steps can have reached.
      def restored(steps, snapshot, scope)
        native_restored(steps, Wire.snapshot_row(snapshot), scope)
      end

      private :native_with_scope, :native_restored, :native_scope_of, :native_from_bytes
    end

    def id = snapshot.id
    def scope = snapshot.scope
    def issued = snapshot.issued
    def progress = snapshot.progress
    def settled = snapshot.settled

    def snapshot = Wire.snapshot(native_snapshot)

    def pending
      row = native_pending
      row && Wire.action(row)
    end

    # An Action, nil once the saga is complete or aborted, or :exhausted.
    def next_action
      answer = native_next_action
      answer.is_a?(Array) ? Wire.action(answer) : answer
    end

    def resolve(action, outcome)
      native_resolve(Wire.action_row(action), outcome)
    end
  end
end
